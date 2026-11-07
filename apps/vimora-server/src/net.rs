//! Console networking: LAN beacon, TCP listener, per-student sessions and the
//! "show teacher screen" broadcaster.

use crate::config::ServerConfig;
use anyhow::{bail, Result};
use eframe::egui;
use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tracing::{info, warn};
use vimora_core::{
    auth, capture,
    net::{read_msg, write_msg},
    Beacon, ClientInfo, ClientMsg, ClientStatus, ServerMsg, StreamConfig, DISCOVERY_PORT,
    PROTOCOL_VERSION,
};
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

pub struct Student {
    pub addr: SocketAddr,
    pub info: ClientInfo,
    pub status: ClientStatus,
    pub frame: Option<egui::ColorImage>,
    pub frame_seq: u64,
    pub frame_size: (u32, u32),
    pub tx: mpsc::Sender<ServerMsg>,
    pub connected_at: Instant,
    pub help: Option<String>,
    /// Last alert (e.g. blocked keyword searched) and when it happened.
    pub alert: Option<(String, Instant)>,
    /// Session binding for signed privileged commands.
    pub client_nonce: Vec<u8>,
    pub priv_counter: u64,
    /// Last reported installed applications.
    pub apps: Option<Vec<vimora_core::AppInfo>>,
    /// Shadow overlay content as last reported.
    pub changes: Option<Vec<vimora_core::ChangeInfo>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
}

/// A short result message shown over the console (command outcomes).
pub struct Toast {
    pub ok: bool,
    pub text: String,
    pub at: Instant,
}

pub struct LogEntry {
    pub time: String,
    pub level: Level,
    pub text: String,
}

/// One message in the teacher ↔ students chat.
pub struct ChatMsg {
    pub time: String,
    /// Sent by the teacher (true) or a student's reply (false).
    pub outgoing: bool,
    /// Recipients ("12 ta kompyuter") for outgoing, the student's name for replies.
    pub who: String,
    pub text: String,
    /// A notice in the middle of the chat ("Chat yakunlandi").
    pub system: bool,
}

pub struct Hub {
    /// Signs sessions and privileged commands; student PCs pin its public key.
    pub identity: Arc<auth::Identity>,
    pub students: BTreeMap<u64, Student>,
    pub log: VecDeque<LogEntry>,
    pub toasts: VecDeque<Toast>,
    pub chat: VecDeque<ChatMsg>,
    /// Computers that received a message in the current chat (they get `ChatEnd`).
    pub chat_members: std::collections::BTreeSet<u64>,
    /// Student replies not seen yet (badge on the "Xabar" menu item).
    pub unread: usize,
    pub ctx: Option<egui::Context>,
    next_id: u64,
}

impl Hub {
    pub fn new(identity: auth::Identity) -> Self {
        Self {
            identity: Arc::new(identity),
            students: BTreeMap::new(),
            log: VecDeque::new(),
            toasts: VecDeque::new(),
            chat: VecDeque::new(),
            chat_members: Default::default(),
            unread: 0,
            ctx: None,
            next_id: 0,
        }
    }

    pub fn toast(&mut self, ok: bool, text: impl Into<String>) {
        self.toasts.push_back(Toast {
            ok,
            text: text.into(),
            at: Instant::now(),
        });
        while self.toasts.len() > 5 {
            self.toasts.pop_front();
        }
        self.repaint();
    }

    pub fn log(&mut self, level: Level, text: impl Into<String>) {
        let text = text.into();
        info!("{text}");
        self.log.push_front(LogEntry {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            text,
        });
        self.log.truncate(300);
        self.repaint();
    }

    pub fn chat(&mut self, outgoing: bool, who: impl Into<String>, text: impl Into<String>) {
        self.chat.push_back(ChatMsg {
            time: chrono::Local::now().format("%H:%M").to_string(),
            outgoing,
            who: who.into(),
            text: text.into(),
            system: false,
        });
        while self.chat.len() > 500 {
            self.chat.pop_front();
        }
        if !outgoing {
            self.unread += 1;
        }
        self.repaint();
    }

    /// Ends the chat on every student's screen (their panel closes until the next message).
    pub fn end_chat(&mut self) {
        if self.chat_members.is_empty() {
            return;
        }
        let ids: Vec<u64> = std::mem::take(&mut self.chat_members).into_iter().collect();
        self.send(&ids, &ServerMsg::Command(vimora_core::Command::ChatEnd));
        self.chat.push_back(ChatMsg {
            time: chrono::Local::now().format("%H:%M").to_string(),
            outgoing: true,
            who: String::new(),
            text: tr("Chat yakunlandi").into(),
            system: true,
        });
        self.repaint();
    }

    pub fn repaint(&self) {
        if let Some(c) = &self.ctx {
            c.request_repaint();
        }
    }

    /// Signs `cmd` per student (session nonce + counter) and sends it.
    pub fn send_priv(&mut self, ids: &[u64], cmd: &vimora_core::PrivCommand) -> usize {
        let Ok(payload) = bincode::serialize(cmd) else {
            return 0;
        };
        let mut n = 0;
        for id in ids {
            if let Some(s) = self.students.get_mut(id) {
                s.priv_counter += 1;
                let sig = self
                    .identity
                    .sign_priv(&s.client_nonce, s.priv_counter, &payload);
                let msg = ServerMsg::Privileged {
                    counter: s.priv_counter,
                    payload: payload.clone(),
                    sig,
                };
                if s.tx.try_send(msg).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }

    /// Sends to the given students; returns how many accepted the message.
    pub fn send(&self, ids: &[u64], msg: &ServerMsg) -> usize {
        ids.iter()
            .filter_map(|id| self.students.get(id))
            .filter(|s| s.tx.try_send(msg.clone()).is_ok())
            .count()
    }
}

pub type SharedHub = Arc<Mutex<Hub>>;
pub type SharedConfig = Arc<RwLock<ServerConfig>>;

#[derive(Default)]
pub struct Show {
    pub active: AtomicBool,
    pub targets: Mutex<Vec<u64>>,
}

pub fn thumb_stream(cfg: &ServerConfig) -> StreamConfig {
    StreamConfig {
        interval_ms: cfg.thumb_interval_ms.max(250),
        ..StreamConfig::THUMBNAIL
    }
}

/// Starts all network services on a background Tokio runtime.
pub fn start(hub: SharedHub, cfg: SharedConfig, show: Arc<Show>) {
    let console_key = hub.lock().unwrap().identity.public();
    let hub2 = hub.clone();
    std::thread::Builder::new()
        .name("vimora-show".into())
        .spawn(move || show_loop(hub2, show))
        .expect("spawn show thread");

    std::thread::Builder::new()
        .name("vimora-net".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                tokio::spawn(beacon_task(cfg.clone(), console_key));
                if let Err(e) = listen(hub.clone(), cfg).await {
                    hub.lock().unwrap().log(
                        Level::Warn,
                        tf!("Tarmoq xatosi: {e}", e = format!("{:#}", e)),
                    );
                }
            });
        })
        .expect("spawn net thread");
}

async fn beacon_task(cfg: SharedConfig, console_key: auth::ConsoleKey) {
    let sock = match UdpSocket::bind(("0.0.0.0", 0)).await {
        Ok(s) => s,
        Err(e) => return warn!("beacon socket: {e}"),
    };
    let _ = sock.set_broadcast(true);
    loop {
        let b = {
            let c = cfg.read().unwrap();
            Beacon {
                version: PROTOCOL_VERSION,
                server_name: c.server_name.clone(),
                tcp_port: c.port,
                console_key,
            }
        };
        if let Ok(pkt) = vimora_core::net::encode_beacon(&b) {
            if let Err(e) = sock
                .send_to(&pkt, ("255.255.255.255", DISCOVERY_PORT))
                .await
            {
                warn!("beacon: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn listen(hub: SharedHub, cfg: SharedConfig) -> Result<()> {
    let port = cfg.read().unwrap().port;
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    hub.lock()
        .unwrap()
        .log(Level::Ok, tf!("Konsol tayyor: TCP {port}", port = port));
    loop {
        let (stream, addr) = listener.accept().await?;
        let (hub, cfg) = (hub.clone(), cfg.clone());
        tokio::spawn(async move {
            if let Err(e) = session(stream, addr, hub.clone(), cfg).await {
                warn!("{addr}: {e:#}");
            }
        });
    }
}

async fn session(
    stream: TcpStream,
    addr: SocketAddr,
    hub: SharedHub,
    cfg: SharedConfig,
) -> Result<()> {
    stream.set_nodelay(true)?;
    let (mut rd, mut wr) = stream.into_split();

    let identity = hub.lock().unwrap().identity.clone();
    let server_nonce = auth::nonce();
    write_msg(
        &mut wr,
        &ServerMsg::Hello {
            version: PROTOCOL_VERSION,
            nonce: server_nonce.clone(),
            console_key: identity.public(),
        },
    )
    .await?;
    let ClientMsg::Hello {
        version,
        nonce: client_nonce,
        info,
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd)).await??
    else {
        bail!("bad hello");
    };
    if version != PROTOCOL_VERSION {
        hub.lock().unwrap().log(
            Level::Warn,
            tf!(
                "{addr}: protokol versiyasi mos emas (v{version})",
                addr = addr,
                version = version
            ),
        );
        bail!("version mismatch");
    }
    // Any agent on the network may join; the agent checks *our* signature and refuses to
    // be controlled by a console other than the one it paired with.
    let stream_cfg = thumb_stream(&cfg.read().unwrap());
    write_msg(
        &mut wr,
        &ServerMsg::Welcome {
            sig: identity.sign_welcome(&client_nonce, &server_nonce),
            stream: stream_cfg,
        },
    )
    .await?;

    let (tx, mut rx) = mpsc::channel::<ServerMsg>(64);
    let id = {
        let mut h = hub.lock().unwrap();
        h.next_id += 1;
        let id = h.next_id;
        h.students.insert(
            id,
            Student {
                addr,
                info: info.clone(),
                status: ClientStatus::default(),
                frame: None,
                frame_seq: 0,
                frame_size: (0, 0),
                tx: tx.clone(),
                connected_at: Instant::now(),
                help: None,
                alert: None,
                client_nonce: client_nonce.clone(),
                priv_counter: 0,
                apps: None,
                changes: None,
            },
        );
        h.log(
            Level::Ok,
            tf!(
                "{v0} ulandi ({v1}, {v2})",
                v0 = info.label(),
                v1 = info.username,
                v2 = addr.ip()
            ),
        );
        id
    };
    {
        // Remember every computer ever seen (navbar shows total vs online).
        let mut c = cfg.write().unwrap();
        if c.known
            .insert(info.key(), info.label().to_string())
            .is_none()
        {
            crate::config::save(&c);
        }
    }

    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_msg(&mut wr, &m).await.is_err() {
                break;
            }
        }
    });
    let pinger = {
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if tx.send(ServerMsg::Ping).await.is_err() {
                    break;
                }
            }
        })
    };

    let result: Result<()> = async {
        loop {
            let msg: ClientMsg =
                tokio::time::timeout(Duration::from_secs(20), read_msg(&mut rd)).await??;
            match msg {
                ClientMsg::Frame {
                    width,
                    height,
                    jpeg,
                } => {
                    let img =
                        tokio::task::spawn_blocking(move || capture::decode_jpeg(&jpeg)).await?;
                    if let Ok(f) = img {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [f.width as usize, f.height as usize],
                            &f.rgba,
                        );
                        let mut h = hub.lock().unwrap();
                        if let Some(s) = h.students.get_mut(&id) {
                            s.frame = Some(img);
                            s.frame_seq += 1;
                            s.frame_size = (width, height);
                        }
                        h.repaint();
                    }
                }
                ClientMsg::Status(st) => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.status = st;
                    }
                    h.repaint();
                }
                ClientMsg::CommandResult { ok, message } => {
                    if message.is_empty() {
                        continue;
                    }
                    let label = info.label().to_string();
                    let mut h = hub.lock().unwrap();
                    h.log(
                        if ok { Level::Info } else { Level::Warn },
                        format!("{label}: {message}"),
                    );
                    h.toast(ok, format!("{label}: {message}"));
                }
                ClientMsg::HelpRequest { text } => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.help = Some(text.clone());
                    }
                    h.log(
                        Level::Warn,
                        tf!(
                            "{v0} yordam so'rayapti: {text}",
                            v0 = info.label(),
                            text = text
                        ),
                    );
                }
                ClientMsg::Alert { text } => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.alert = Some((text.clone(), Instant::now()));
                    }
                    h.log(Level::Warn, format!("{}: {text}", info.label()));
                }
                ClientMsg::Apps(list) => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.apps = Some(list);
                    }
                    h.repaint();
                }
                ClientMsg::Changes(list) => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.changes = Some(list);
                    }
                    h.repaint();
                }
                ClientMsg::Chat { text } => {
                    hub.lock().unwrap().chat(false, info.label(), text);
                }
                ClientMsg::Pong | ClientMsg::Hello { .. } => {}
            }
        }
    }
    .await;

    writer.abort();
    pinger.abort();
    let mut h = hub.lock().unwrap();
    h.students.remove(&id);
    h.log(Level::Info, tf!("{v0} uzildi", v0 = info.label()));
    result
}

/// Captures the teacher's screen and streams it to the current show targets.
fn show_loop(hub: SharedHub, show: Arc<Show>) {
    let mut cap: Option<capture::Capturer> = None;
    let mut last = Vec::new();
    let mut last_sent = Instant::now();
    loop {
        if !show.active.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if cap.is_none() {
            match capture::Capturer::new() {
                Ok(c) => cap = Some(c),
                Err(e) => {
                    hub.lock().unwrap().log(
                        Level::Warn,
                        tf!("Ekranni olish imkonsiz: {e}", e = format!("{:#}", e)),
                    );
                    show.active.store(false, Ordering::Relaxed);
                    continue;
                }
            }
        }
        let t0 = Instant::now();
        let frame = cap
            .as_mut()
            .unwrap()
            .capture()
            .and_then(|f| capture::encode_jpeg(&f, 1920, 70));
        match frame {
            Ok((width, height, jpeg)) => {
                if jpeg != last || last_sent.elapsed() > Duration::from_secs(1) {
                    last_sent = Instant::now();
                    last = jpeg.clone();
                    let targets = show.targets.lock().unwrap().clone();
                    hub.lock().unwrap().send(
                        &targets,
                        &ServerMsg::ShowFrame {
                            width,
                            height,
                            jpeg,
                        },
                    );
                }
            }
            Err(e) => {
                // Stop presenting instead of leaving students on a black screen.
                hub.lock().unwrap().log(
                    Level::Warn,
                    tf!(
                        "Namoyish to'xtatildi — ekranni olib bo'lmadi: {e}",
                        e = format!("{:#}", e)
                    ),
                );
                show.active.store(false, Ordering::Relaxed);
                cap = None;
            }
        }
        std::thread::sleep(Duration::from_millis(100).saturating_sub(t0.elapsed()));
    }
}
