//! Headless agent: discovery, authenticated session with the teacher, command execution
//! and supervision of the overlay process.

mod chat_supervisor;
mod lesson_supervisor;
mod overlay_supervisor;
mod session;
mod tasks;
mod tray;

use crate::config::Config;

use crate::netcontrol;

use crate::privileged::Engine;

use crate::service::{self, IpcReq, IpcResp};

use crate::overlay::OverlayCmd;

use crate::system;

use anyhow::{bail, Context, Result};

use std::process::Stdio;

use std::sync::atomic::{AtomicBool, Ordering};

use std::sync::{Arc, Mutex};

use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;

use tokio::net::{TcpStream, UdpSocket};

use tokio::process::{Child, ChildStdin};

use tokio::sync::mpsc;

use tracing::{info, warn};

use chat_supervisor::SharedChat;
use overlay_supervisor::*;
use session::*;
use tasks::*;
use vimora_core::{
    auth, capture, input,
    net::{read_msg, write_msg},
    Command, FreezeState, NetPolicy, PowerAction, PrivCommand, StreamConfig, StudentInfo,
    StudentMsg, StudentStatus, TeacherMsg, DISCOVERY_PORT, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
};

/// Teacher computers' names heard in their beacons ("ip:port" → name), until the teacher
/// program says its name itself (`Command::TeacherInfo`).
pub(crate) static TEACHER_NAMES: Mutex<std::collections::BTreeMap<String, String>> =
    Mutex::new(std::collections::BTreeMap::new());

#[derive(Default)]
struct State {
    lock: Option<(String, String)>,
    /// Offline unlock code of the current lock: (salt, hash, digits).
    unlock: Option<(Vec<u8>, Vec<u8>, u8)>,
    /// Icon for the lock screen (empty = default).
    lock_icon: String,
    show: bool,
    apps: Vec<String>,
    /// Only these programs may run (Windows; empty = off).
    allow_apps: Vec<String>,
    net: NetPolicy,
    help: bool,
    connected: bool,
    /// Name of the teacher's computer (the last one connected), for the tray window.
    teacher: Option<String>,
    disconnected_at: Option<Instant>,
    service: bool,
    freeze: FreezeState,
    shield_usage: Option<vimora_core::ShieldUsage>,
    shield_note: Option<String>,
    os_updates: Option<u32>,
    app_updates: Option<u32>,
    busy: Option<String>,
    queued: u32,
    persistent_folders: Vec<String>,
}

impl State {
    fn status(&self) -> StudentStatus {
        StudentStatus {
            locked: self.lock.is_some(),
            showing_teacher: self.show,
            app_blocklist: self.apps.len(),
            web_blocklist: self.net.domains.len(),
            keywords: self.net.keywords.len(),
            safe_search: self.net.safe_search,
            help_requested: self.help,
            service: self.service,
            freeze: self.freeze,
            shield_usage: self.shield_usage,
            shield_note: self.shield_note.clone(),
            os_updates: self.os_updates,
            app_updates: self.app_updates,
            busy: self.busy.clone(),
            queued: self.queued,
            persistent_folders: self.persistent_folders.clone(),
        }
    }
}

type Shared = Arc<Mutex<State>>;

/// Things that outlive a single teacher session.
#[derive(Clone)]
struct Ctx {
    state: Shared,
    /// Local executor, used only when no Vimora service is running (dev / unprivileged mode).
    engine: Arc<Engine>,
    /// The teacher this PC follows (public key), once paired.
    pin: Arc<Mutex<Option<vimora_core::auth::TeacherKey>>>,
    /// Student nonce + last privileged counter of the current session (local verification).
    session: Arc<Mutex<(Vec<u8>, u64)>>,
    /// Sender of the current session (None while disconnected).
    uplink: Arc<Mutex<Option<mpsc::Sender<StudentMsg>>>>,
    /// The docked chat panel, while a chat is open.
    chat: SharedChat,
}

/// Seconds since the epoch of the agent's last sign of life (watched by `main`'s watchdog).
pub static HEARTBEAT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Set when the teacher removed this PC (`PrivCommand::Unpair`): the session ends and that
/// teacher is left alone for a while.
pub(crate) static UNPAIRED: Mutex<bool> = Mutex::new(false);

/// Things the teacher should hear about (a window that cannot open…); sent as alerts by
/// `service_status_task` once a teacher is connected.
pub(crate) static FAULTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn report_fault(text: impl Into<String>) {
    let text = text.into();
    warn!("{text}");
    let mut f = FAULTS.lock().unwrap();
    if f.len() < 8 && !f.contains(&text) {
        f.push(text);
    }
}

/// Finds a graphics backend that can actually draw on this PC before any window is needed:
/// runs `vimora-student ui-probe` with each one and keeps the first that works.
async fn probe_ui() {
    if std::env::var_os("VIMORA_RENDERER").is_some() {
        return; // chosen by hand
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    for r in ["glow", "wgpu"] {
        let child = tokio::process::Command::new(&exe)
            .arg("ui-probe")
            .env("VIMORA_RENDERER", r)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let Ok(mut child) = child else { continue };
        let ok = matches!(
            tokio::time::timeout(Duration::from_secs(20), child.wait()).await,
            Ok(Ok(st)) if st.success()
        );
        if ok {
            info!("windows are drawn with {r}");
            std::env::set_var("VIMORA_RENDERER", r);
            return;
        }
        let _ = child.kill().await;
        warn!("graphics backend {r} does not work on this PC");
    }
    report_fault("Bu kompyuterda oynalarni chizib bo'lmadi (grafik drayver?) — qulf ekrani va xabarlar ishlamasligi mumkin");
}

/// Runs a background task and starts it again if it ever panics or returns: no helper may
/// silently disappear and leave the agent half-working.
fn keep_alive<F, Fut>(name: &'static str, make: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            if let Err(e) = tokio::spawn(make()).await {
                warn!("background task '{name}' failed ({e}); restarting it");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

pub async fn run(cfg: Config) -> Result<()> {
    info!("Vimora agent starting");
    let state: Shared = Arc::default();
    let ctx = Ctx {
        state: state.clone(),
        engine: Engine::new(),
        pin: Arc::new(Mutex::new(None)),
        session: Arc::default(),
        uplink: Arc::default(),
        chat: Arc::default(),
    };
    // A lock desktop left behind by a crash: show the normal one (a lock still in force opens
    // again right away).
    crate::desktop::restore();
    restore_lock(&state);
    restore_apps(&state);
    // The folder students save their work into ("Topshiriq"), ready before it is collected.
    let _ = system::assignment_dir();
    tray::start(state.clone());
    probe_ui().await;
    let (ov_tx, ov_rx) = mpsc::channel(16);
    tokio::spawn(overlay_task(state.clone(), ov_rx));
    {
        let s = state.clone();
        keep_alive("blocker", move || blocker_task(s.clone()));
    }
    keep_alive("update-restart", restart_on_update);
    // Sound off by the teacher ("Qurilmalar"): kept muted.
    crate::audio::load();
    tokio::spawn(async {
        loop {
            let _ = tokio::task::spawn_blocking(crate::audio::enforce).await;
            tokio::time::sleep(Duration::from_secs(8)).await;
        }
    });
    {
        let e = ctx.engine.clone();
        keep_alive("net-enforce", move || net_enforce_task(e.clone()));
    }
    {
        let c = ctx.clone();
        keep_alive("service-status", move || service_status_task(c.clone()));
    }
    {
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("keyword-watch".into())
            .spawn(move || keyword_watch(ctx))?;
    }
    {
        let (s, tx, secs) = (
            state.clone(),
            ov_tx.clone(),
            cfg.unlock_after_disconnect_secs,
        );
        keep_alive("auto-unlock", move || {
            auto_unlock_task(s.clone(), tx.clone(), secs)
        });
    }
    // Proof of life for the watchdog thread in `main`.
    tokio::spawn(async {
        loop {
            HEARTBEAT.store(now_secs(), Ordering::Relaxed);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    let mut failures = 0u32;
    // Teachers that turned this PC away (or that it was just removed from): left alone for a
    // while, so another teacher on the network gets its turn.
    let mut ignored: std::collections::HashMap<String, Instant> = Default::default();
    loop {
        let pin = refresh_pin(&ctx).await;
        ignored.retain(|_, t| t.elapsed() < Duration::from_secs(90));
        let skip: Vec<String> = ignored.keys().cloned().collect();
        // Where to connect: a fixed address from the config, else the teacher that worked last
        // time (tried first), else whatever announces itself on the network.
        let mut targets: Vec<String> = vec![];
        // Not paired with any teacher (a new install, or removed from its teacher): free. It
        // shows itself to every teacher ("Skanerlash") and joins the one whose teacher adds it.
        if pin.is_none() && cfg.server.is_none() {
            let addr = free_mode(cfg.name.clone()).await;
            targets.push(addr);
        }
        match &cfg.server {
            Some(s) => targets.push(s.clone()),
            None if !targets.is_empty() => {}
            None => {
                if let Some(last) = crate::config::last_teacher().filter(|l| !skip.contains(l)) {
                    targets.push(last);
                }
                if let Ok(found) = tokio::time::timeout(
                    // A teacher that turned this PC away is skipped only for a while: look again
                    // every minute (an hour-long wait kept removed PCs out of sight).
                    Duration::from_secs(if targets.is_empty() { 60 } else { 6 }),
                    discover(pin, skip.clone(), my_info(cfg.name.clone()).key()),
                )
                .await
                {
                    if !targets.contains(&found) {
                        targets.insert(0, found);
                    }
                }
            }
        }
        let mut connected = false;
        for addr in targets {
            info!("connecting to teacher {addr}");
            // The session runs as its own task, so even a panic in it only ends that session.
            let (c, a, x, o) = (cfg.clone(), addr.clone(), ctx.clone(), ov_tx.clone());
            let started = Instant::now();
            match tokio::spawn(async move { session(&c, &a, &x, &o).await }).await {
                Ok(Ok(())) => info!("teacher closed the connection"),
                Ok(Err(e)) => warn!("session ended: {e:#}"),
                Err(e) => warn!("session crashed: {e}"),
            }
            let real = started.elapsed() > Duration::from_secs(10);
            connected |= real;
            // Turned away at the door, or unpaired during the session: give others a chance.
            if !real || std::mem::take(&mut *UNPAIRED.lock().unwrap()) {
                ignored.insert(addr.clone(), Instant::now());
            }
            *ctx.uplink.lock().unwrap() = None;
            {
                let mut s = state.lock().unwrap();
                s.connected = false;
                s.disconnected_at = Some(Instant::now());
                s.show = false;
            }
            let _ = ov_tx.send(OverlayReq::Sync).await;
            if connected {
                break;
            }
        }
        // Retry quickly after a real session (the teacher may just have restarted), a little
        // slower when nothing could be reached.
        failures = if connected { 0 } else { (failures + 1).min(5) };
        tokio::time::sleep(Duration::from_secs(2 + failures as u64)).await;
    }
}

/// Free mode: tells every teacher that announces itself "I am free" (`StudentMsg::Free`), every
/// few seconds, until one of them claims this PC; returns that teacher's "ip:port".
fn my_info(name: Option<String>) -> StudentInfo {
    StudentInfo {
        hostname: system::hostname(),
        username: system::username(),
        os: std::env::consts::OS.into(),
        display_name: name,
        machine_id: system::machine_id(),
    }
}

async fn free_mode(name: Option<String>) -> String {
    let info = my_info(name);
    let key = info.key();
    info!("free: waiting for a teacher to add this computer");
    loop {
        let sock = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await {
            Ok(s) => s,
            Err(e) => {
                warn!("cannot listen for discovery on UDP {DISCOVERY_PORT}: {e}");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        let mut teachers: std::collections::BTreeSet<String> = Default::default();
        // The teacher used last (if any) hears "I am free" even without its broadcasts.
        if let Some(last) = crate::config::last_teacher() {
            teachers.insert(last);
        }
        let mut last_hello = Instant::now() - Duration::from_secs(60);
        let mut buf = [0u8; 2048];
        loop {
            // Say hello to every teacher seen so far, every 5 s.
            if last_hello.elapsed() > Duration::from_secs(5) {
                last_hello = Instant::now();
                for addr in teachers.clone() {
                    let info = info.clone();
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(Duration::from_secs(4), say_free(&addr, info))
                            .await;
                    });
                }
            }
            let Ok(Ok((n, src))) =
                tokio::time::timeout(Duration::from_secs(1), sock.recv_from(&mut buf)).await
            else {
                continue;
            };
            if let Some((target, port)) = vimora_core::net::decode_claim(&buf[..n]) {
                if target == key {
                    let addr = format!("{}:{port}", src.ip());
                    info!("free: added by the teacher at {addr}");
                    return addr;
                }
                continue;
            }
            // A teacher pressed "Skanerlash": answer that teacher right away.
            if let Some(port) = vimora_core::net::decode_scan(&buf[..n]) {
                let addr = format!("{}:{port}", src.ip());
                teachers.insert(addr.clone());
                let info = info.clone();
                tokio::spawn(async move {
                    let _ =
                        tokio::time::timeout(Duration::from_secs(4), say_free(&addr, info)).await;
                });
                continue;
            }
            if let Some(b) = vimora_core::net::decode_beacon(&buf[..n]) {
                if b.version >= MIN_PROTOCOL_VERSION {
                    teachers.insert(format!("{}:{}", src.ip(), b.tcp_port));
                }
            }
        }
    }
}

/// One "I am free" visit: the teacher's hello, our `Free`, done.
async fn say_free(addr: &str, info: StudentInfo) -> Result<()> {
    let stream = TcpStream::connect(addr).await?;
    let (mut rd, mut wr) = stream.into_split();
    let _hello: TeacherMsg = read_msg(&mut rd).await?;
    write_msg(&mut wr, &StudentMsg::Free(info)).await?;
    Ok(())
}

/// Wait for a teacher beacon matching our room; returns "ip:port".
/// The teacher this PC follows: the service's pairing is authoritative (it can't be edited by
/// the student); without a service the agent's own copy is used.
async fn refresh_pin(ctx: &Ctx) -> Option<vimora_core::auth::TeacherKey> {
    let pin = match service::call(IpcReq::Pin).await {
        Ok(IpcResp::Pin(p)) => p,
        _ => crate::config::pin(),
    };
    *ctx.pin.lock().unwrap() = pin;
    pin
}

/// Waits for a teacher beacon; once paired, only the paired teacher's beacon counts.
/// A teacher that adds this computer ("Qo'shish", a claim datagram) is answered at once, even
/// one that turned it away before (it was removed there and is now added back).
async fn discover(
    pin: Option<vimora_core::auth::TeacherKey>,
    skip: Vec<String>,
    key: String,
) -> String {
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
        loop {
            let Ok((n, src)) = sock.recv_from(&mut buf).await else {
                break;
            };
            if let Some((target, port)) = vimora_core::net::decode_claim(&buf[..n]) {
                if target == key {
                    info!("added back by the teacher at {}", src.ip());
                    return format!("{}:{port}", src.ip());
                }
                continue;
            }
            let Some(b) = vimora_core::net::decode_beacon(&buf[..n]) else {
                continue;
            };
            if b.version < MIN_PROTOCOL_VERSION || pin.is_some_and(|k| k != b.teacher_key) {
                continue;
            }
            if skip.contains(&format!("{}:{}", src.ip(), b.tcp_port)) {
                continue;
            }
            info!(
                "found teacher '{}' at {} ({})",
                b.server_name,
                src.ip(),
                vimora_core::auth::fingerprint(&b.teacher_key)
            );
            let addr = format!("{}:{}", src.ip(), b.tcp_port);
            TEACHER_NAMES
                .lock()
                .unwrap()
                .insert(addr.clone(), b.server_name.clone());
            return addr;
        }
    }
}

/// After an update replaced our executable on disk, start the new one and quit (the installer
/// renames or unlinks the running file, so this process keeps working until then).
async fn restart_on_update() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let stamp = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let start = stamp(&exe);
    let mut round = 0u32;
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        round += 1;
        // The file we run from changed, or (every minute) the installed program reports another
        // version — the second also works when the installer moved our file aside.
        let changed = start.is_some() && stamp(&exe) != start;
        let other = round.is_multiple_of(2)
            && tokio::task::spawn_blocking(crate::selfcheck::replaced)
                .await
                .ok()
                .flatten()
                .is_some();
        if !(changed || other) {
            continue;
        }
        // Let the installer finish writing the file.
        tokio::time::sleep(Duration::from_secs(10)).await;
        let target = crate::selfcheck::installed_exe().unwrap_or_else(|| exe.clone());
        let args: Vec<String> = std::env::args().skip(1).collect();
        match std::process::Command::new(&target).args(&args).spawn() {
            Ok(_) => {
                info!("Vimora was updated — restarting");
                std::process::exit(0);
            }
            Err(e) => warn!("cannot start the updated agent: {e}"),
        }
    }
}

// --- the lock survives an agent restart --------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedLock {
    title: String,
    message: String,
    code: Option<(Vec<u8>, Vec<u8>, u8)>,
    #[serde(default)]
    icon: String,
    /// Unix seconds when it was saved.
    at: u64,
}

fn lock_file() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("vimora").join("lock-state.json"))
}

/// Writes (or removes) the lock record: if the agent restarts while the screen is locked, the
/// lock comes straight back instead of quietly disappearing.
fn save_lock(state: &Shared) {
    let Some(path) = lock_file() else { return };
    let s = state.lock().unwrap();
    match &s.lock {
        Some((title, message)) => {
            let rec = SavedLock {
                title: title.clone(),
                message: message.clone(),
                code: s.unlock.clone(),
                icon: s.lock_icon.clone(),
                at: now_secs(),
            };
            if let Some(d) = path.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(j) = serde_json::to_string(&rec) {
                let _ = std::fs::write(&path, j);
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Brings back a lock saved before a restart (not older than 12 hours).
fn restore_lock(state: &Shared) {
    let Some(rec) = lock_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|j| serde_json::from_str::<SavedLock>(&j).ok())
        .filter(|r| now_secs().saturating_sub(r.at) < 12 * 3600)
    else {
        return;
    };
    let mut s = state.lock().unwrap();
    info!("restoring the lock screen after a restart");
    s.lock = Some((rec.title, rec.message));
    s.unlock = rec.code;
    s.lock_icon = rec.icon;
    s.disconnected_at = Some(Instant::now());
}

/// Program block / allow lists, kept across agent restarts (they apply with no teacher too).
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedApps {
    block: Vec<String>,
    allow: Vec<String>,
}

fn apps_file() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("vimora").join("app-rules.json"))
}

fn save_apps(state: &Shared) {
    let Some(path) = apps_file() else { return };
    let rec = {
        let s = state.lock().unwrap();
        SavedApps {
            block: s.apps.clone(),
            allow: s.allow_apps.clone(),
        }
    };
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(j) = serde_json::to_string(&rec) {
        let _ = std::fs::write(path, j);
    }
}

fn restore_apps(state: &Shared) {
    if let Some(rec) = apps_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|j| serde_json::from_str::<SavedApps>(&j).ok())
    {
        let mut s = state.lock().unwrap();
        s.apps = rec.block;
        s.allow_apps = rec.allow;
    }
}
