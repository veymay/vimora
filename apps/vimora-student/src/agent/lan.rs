//! What the agent hears on the network, kept in one place.
//!
//! One task listens on the discovery port for good (teacher beacons, "scan" calls, "take this
//! computer" claims) and remembers what it heard. Finding a teacher is then a look at fresh
//! beacons instead of waiting for the next one to fly by, so a PC finds its teacher again at
//! once after the network changed (a new Wi-Fi, a cable, a new address).
//!
//! The socket is bound again whenever it has been silent for a while: after a network change
//! some systems leave a listening socket deaf.

use super::*;
use std::collections::{HashMap, VecDeque};
use std::sync::LazyLock;
use tokio::sync::Notify;
use vimora_core::protocol::Beacon;

/// A beacon is current for this long (teachers send one every 2 s).
const FRESH: Duration = Duration::from_secs(7);
/// Bind the socket again after this long without a single datagram.
const DEAF_AFTER: Duration = Duration::from_secs(20);

#[derive(Default)]
struct Heard {
    /// "ip:port" of a teacher → when its beacon came and what it said.
    teachers: HashMap<String, (Instant, Beacon)>,
    /// Teachers that called "scan" (they want to hear from free computers): "ip:port".
    scans: VecDeque<(Instant, String)>,
    /// Teachers that said "take this computer" to us: "ip:port".
    claims: VecDeque<(Instant, String)>,
}

static HEARD: LazyLock<Mutex<Heard>> = LazyLock::new(|| Mutex::new(Heard::default()));
static NEWS: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Starts the listener (once per agent); `key` is this computer's own key, for claims.
pub(super) fn start(key: String) {
    tokio::spawn(async move {
        loop {
            let sock = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("cannot listen for discovery on UDP {DISCOVERY_PORT}: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };
            let mut buf = [0u8; 2048];
            let mut last_heard = Instant::now();
            loop {
                let got =
                    tokio::time::timeout(Duration::from_secs(2), sock.recv_from(&mut buf)).await;
                let (n, src) = match got {
                    Ok(Ok(x)) => x,
                    Ok(Err(_)) => break, // a broken socket: bind again
                    Err(_) => {
                        if last_heard.elapsed() > DEAF_AFTER {
                            break; // silent for too long: maybe deaf after a network change
                        }
                        continue;
                    }
                };
                last_heard = Instant::now();
                absorb(&buf[..n], src.ip(), &key);
            }
            drop(sock);
            // A moment, then the same port again (it may take the system a moment to free it).
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });
}

fn absorb(data: &[u8], from: std::net::IpAddr, key: &str) {
    let now = Instant::now();
    let mut h = HEARD.lock().unwrap();
    if let Some((target, port)) = vimora_core::net::decode_claim(data) {
        if target == key {
            h.claims.push_back((now, format!("{from}:{port}")));
            while h.claims.len() > 8 {
                h.claims.pop_front();
            }
        }
    } else if let Some(port) = vimora_core::net::decode_scan(data) {
        h.scans.push_back((now, format!("{from}:{port}")));
        while h.scans.len() > 16 {
            h.scans.pop_front();
        }
    } else if let Some(b) = vimora_core::net::decode_beacon(data) {
        if b.version >= MIN_PROTOCOL_VERSION {
            let addr = format!("{from}:{}", b.tcp_port);
            TEACHER_NAMES
                .lock()
                .unwrap()
                .insert(addr.clone(), b.server_name.clone());
            h.teachers.insert(addr, (now, b));
        }
        h.teachers
            .retain(|_, (t, _)| t.elapsed() < Duration::from_secs(60));
    } else {
        return;
    }
    drop(h);
    NEWS.notify_waiters();
}

/// A teacher that "took" this computer (a claim), if one came in the last few seconds.
pub(super) fn take_claim() -> Option<String> {
    let mut h = HEARD.lock().unwrap();
    while let Some((t, addr)) = h.claims.pop_front() {
        if t.elapsed() < Duration::from_secs(10) {
            return Some(addr);
        }
    }
    None
}

/// Teachers that asked for free computers to show themselves since the last call.
pub(super) fn take_scans() -> Vec<String> {
    let mut h = HEARD.lock().unwrap();
    let v: Vec<String> = h
        .scans
        .drain(..)
        .filter(|(t, _)| t.elapsed() < Duration::from_secs(10))
        .map(|(_, a)| a)
        .collect();
    v
}

/// Teachers whose beacon is fresh (any key).
pub(super) fn fresh_teachers() -> Vec<String> {
    let h = HEARD.lock().unwrap();
    h.teachers
        .iter()
        .filter(|(_, (t, _))| t.elapsed() < FRESH)
        .map(|(a, _)| a.clone())
        .collect()
}

/// The best teacher to join now: freshest beacon of one this computer may follow (the paired
/// teacher, or any when unpaired), not skipped.
pub(super) fn best_teacher(
    pin: Option<vimora_core::auth::TeacherKey>,
    skip: &[String],
) -> Option<String> {
    let h = HEARD.lock().unwrap();
    h.teachers
        .iter()
        .filter(|(a, (t, b))| {
            t.elapsed() < FRESH && pin.is_none_or(|k| k == b.teacher_key) && !skip.contains(a)
        })
        .min_by_key(|(_, (t, _))| t.elapsed())
        .map(|(a, _)| a.clone())
}

/// Waits for something new to be heard (or a short time to pass).
pub(super) async fn news(max: Duration) {
    let _ = tokio::time::timeout(max, NEWS.notified()).await;
}
