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
    MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
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
    /// Agent version (empty until it says so; old agents never do).
    pub version: String,
    /// Pointer shape on the student's screen (live view).
    pub cursor: u8,
    /// Last technical report ("Kompyuter holati" drawer) and when it arrived.
    pub sysinfo: Option<(vimora_core::SystemInfo, Instant)>,
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
    /// Outgoing only: who it was sent to, whose panel showed it, who has seen it
    /// (Telegram's clock / one tick / two ticks).
    pub to: Vec<u64>,
    pub delivered: std::collections::BTreeSet<u64>,
    pub read: std::collections::BTreeSet<u64>,
}

/// What the student agent answers when the chat panel took the message.
pub const CHAT_DELIVERED: &str = "xabar yetkazildi";

pub struct Hub {
    /// Signs sessions and privileged commands; student PCs pin its public key.
    pub identity: Arc<auth::Identity>,
    pub students: BTreeMap<u64, Student>,
    pub log: VecDeque<LogEntry>,
    pub toasts: VecDeque<Toast>,
    /// Sounds waiting to be played by the UI thread.
    pub sounds: Vec<crate::sound::Sound>,
    pub chat: VecDeque<ChatMsg>,
    /// Computers that received a message in the current chat (they get `ChatEnd`).
    pub chat_members: std::collections::BTreeSet<u64>,
    /// Student replies not seen yet (badge on the "Xabar" menu item).
    pub unread: usize,
    pub ctx: Option<egui::Context>,
    /// Removed computers already told (this run) to forget this console.
    pub unpaired: std::collections::HashSet<String>,
    next_id: u64,
}

impl Hub {
    pub fn new(identity: auth::Identity) -> Self {
        Self {
            identity: Arc::new(identity),
            students: BTreeMap::new(),
            log: VecDeque::new(),
            toasts: VecDeque::new(),
            sounds: Vec::new(),
            chat: VecDeque::new(),
            chat_members: Default::default(),
            unread: 0,
            ctx: None,
            unpaired: Default::default(),
            next_id: 0,
        }
    }

    pub fn toast(&mut self, ok: bool, text: impl Into<String>) {
        use crate::sound::Sound;
        let snd = if ok { Sound::Ok } else { Sound::Warn };
        if self.sounds.len() < 4 && !self.sounds.contains(&snd) {
            self.sounds.push(snd);
        }
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
            to: vec![],
            delivered: Default::default(),
            read: Default::default(),
        });
        while self.chat.len() > 500 {
            self.chat.pop_front();
        }
        if !outgoing {
            self.unread += 1;
        }
        self.repaint();
    }

    /// A teacher's message to `to` (delivery and read ticks are tracked per computer).
    pub fn chat_out(&mut self, who: impl Into<String>, text: impl Into<String>, to: Vec<u64>) {
        self.chat(true, who, text);
        if let Some(m) = self.chat.back_mut() {
            m.to = to;
        }
    }

    /// The oldest of `id`'s messages its panel hasn't confirmed yet is now delivered.
    fn chat_delivered(&mut self, id: u64) {
        if let Some(m) = self
            .chat
            .iter_mut()
            .find(|m| m.outgoing && m.to.contains(&id) && !m.delivered.contains(&id))
        {
            m.delivered.insert(id);
        }
        self.repaint();
    }

    /// The student saw the panel: everything sent to them so far is read.
    fn chat_read(&mut self, id: u64) {
        for m in self
            .chat
            .iter_mut()
            .filter(|m| m.outgoing && m.to.contains(&id))
        {
            m.delivered.insert(id);
            m.read.insert(id);
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
            to: vec![],
            delivered: Default::default(),
            read: Default::default(),
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
    if version < MIN_PROTOCOL_VERSION {
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
    // A computer the teacher removed from this console is not let back in (it stays free for
    // another console) until it is restored in the settings.
    let removed = cfg.read().unwrap().removed.contains(&info.key());
    if removed {
        // It was removed while offline and may still be paired with us: once per run, let it
        // in just long enough to receive a signed "forget this console", so that another
        // console can take it. Afterwards it is turned away.
        let first = hub.lock().unwrap().unpaired.insert(info.key());
        if first {
            let payload = bincode::serialize(&vimora_core::PrivCommand::Unpair)?;
            let stream = thumb_stream(&cfg.read().unwrap());
            write_msg(
                &mut wr,
                &ServerMsg::Welcome {
                    sig: identity.sign_welcome(&client_nonce, &server_nonce),
                    stream,
                },
            )
            .await?;
            write_msg(
                &mut wr,
                &ServerMsg::Privileged {
                    counter: 1,
                    sig: identity.sign_priv(&client_nonce, 1, &payload),
                    payload,
                },
            )
            .await?;
            // Give the agent a moment to act on it before the connection closes.
            let _ = tokio::time::timeout(Duration::from_secs(3), read_msg::<_, ClientMsg>(&mut rd))
                .await;
            hub.lock().unwrap().log(
                Level::Info,
                tf!("{v0} konsoldan ajratildi", v0 = info.label()),
            );
        }
        bail!("removed computer {} turned away", info.label());
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
                sysinfo: None,
                version: String::new(),
                cursor: 0,
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
        // Remember every computer ever seen: it stays in the console (offline) after it
        // disconnects, until the teacher removes it.
        let mut c = cfg.write().unwrap();
        let entry = crate::config::KnownPc {
            label: info.label().to_string(),
            hostname: info.hostname.clone(),
            user: info.username.clone(),
            os: info.os.clone(),
            ip: addr.ip().to_string(),
            version: c
                .known_pcs
                .get(&info.key())
                .map(|k| k.version.clone())
                .unwrap_or_default(),
            mac: info.machine_id.clone(),
            last_seen: unix_now(),
        };
        let a = c.known.insert(info.key(), info.label().to_string());
        let b = c.known_pcs.insert(info.key(), entry.clone());
        if a.is_none()
            || b.as_ref().is_none_or(|o| {
                o.label != entry.label
                    || o.ip != entry.ip
                    || o.user != entry.user
                    || o.mac != entry.mac
            })
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
                match tokio::time::timeout(Duration::from_secs(20), read_msg(&mut rd)).await? {
                    Ok(m) => m,
                    // A newer agent's message this console doesn't know: skip it, stay connected.
                    Err(e) if vimora_core::net::is_undecodable(&e) => continue,
                    Err(e) => return Err(e),
                };
            if cfg.read().unwrap().removed.contains(&info.key()) {
                return Ok(()); // removed while connected
            }
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
                    // Chat delivery is a tick on the message, not a notification.
                    if ok && message == CHAT_DELIVERED {
                        hub.lock().unwrap().chat_delivered(id);
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
                    h.sounds.push(crate::sound::Sound::Alert);
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
                    h.sounds.push(crate::sound::Sound::Alert);
                    h.log(Level::Warn, format!("{}: {text}", info.label()));
                }
                ClientMsg::SystemInfo(info) => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.sysinfo = Some((info, Instant::now()));
                    }
                    h.repaint();
                }
                ClientMsg::Apps(list) => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.apps = Some(list);
                    }
                    h.repaint();
                }
                ClientMsg::AgentVersion(v) => {
                    let v: String = v.chars().take(24).collect();
                    if let Some(s) = hub.lock().unwrap().students.get_mut(&id) {
                        s.version = v.clone();
                    }
                    let mut c = cfg.write().unwrap();
                    if let Some(k) = c.known_pcs.get_mut(&info.key()) {
                        if k.version != v {
                            k.version = v;
                            crate::config::save(&c);
                        }
                    }
                }
                ClientMsg::Cursor(k) => {
                    let mut h = hub.lock().unwrap();
                    if let Some(s) = h.students.get_mut(&id) {
                        s.cursor = k;
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
                    let mut h = hub.lock().unwrap();
                    // A reply means they have read what came before it.
                    h.chat_read(id);
                    h.chat(false, info.label(), text);
                }
                ClientMsg::ChatRead => hub.lock().unwrap().chat_read(id),
                ClientMsg::Pong | ClientMsg::Hello { .. } => {}
            }
        }
    }
    .await;

    writer.abort();
    pinger.abort();
    {
        let mut c = cfg.write().unwrap();
        if let Some(k) = c.known_pcs.get_mut(&info.key()) {
            k.last_seen = unix_now();
            crate::config::save(&c);
        }
    }
    let mut h = hub.lock().unwrap();
    h.students.remove(&id);
    h.log(Level::Info, tf!("{v0} uzildi", v0 = info.label()));
    result
}

/// Captures the teacher's screen and streams it to the current show targets.
fn show_loop(hub: SharedHub, show: Arc<Show>) {
    let mut cap: Option<capture::Capturer> = None;
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cast: Option<crate::wayland_cast::Cast> = None;
    let mut last = Vec::new();
    #[allow(unused_mut, unused_variables)]
    let mut last_dims = (0u32, 0u32);
    let mut last_sent = Instant::now();
    // Tells the teacher why presenting stopped, in plain words, and stops it.
    let fail = |e: &anyhow::Error| {
        let s = format!("{e:#}");
        let msg = if s.contains("CAST_DENIED") {
            tr("Namoyish bekor qilindi: ekranni ulashishga ruxsat berilmadi.")
        } else if s.contains("NO_GSTREAMER") {
            tr("Namoyish uchun GStreamer kerak: sudo apt install gstreamer1.0-tools gstreamer1.0-pipewire")
        } else if s.contains("PORTAL") || s.contains("CAST_TIMEOUT") || s.contains("WAYLAND") {
            tr("Namoyish boshlanmadi: tizim ekranni ulashishga javob bermadi.")
        } else {
            tr("Namoyish to'xtadi: ekranni olib bo'lmadi.")
        };
        tracing::warn!("show capture failed: {s}");
        let mut h = hub.lock().unwrap();
        h.log(Level::Warn, msg);
        h.toast(false, msg);
        drop(h);
        show.active.store(false, Ordering::Relaxed);
    };
    let send = |width: u32, height: u32, jpeg: Vec<u8>| {
        let targets = show.targets.lock().unwrap().clone();
        hub.lock().unwrap().send(
            &targets,
            &ServerMsg::ShowFrame {
                width,
                height,
                jpeg,
            },
        );
    };
    loop {
        if !show.active.load(Ordering::Relaxed) {
            // Not presenting: let go of the screen (ends the portal session on Wayland).
            cap = None;
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                cast = None;
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let t0 = Instant::now();

        // Wayland: frames come from the desktop portal (see `wayland_cast`).
        #[cfg(all(unix, not(target_os = "macos")))]
        if crate::wayland_cast::is_wayland() {
            if cast.is_none() {
                match crate::wayland_cast::Cast::start() {
                    Ok(c) => {
                        cast = Some(c);
                        last.clear();
                    }
                    Err(e) => {
                        fail(&e);
                        continue;
                    }
                }
            }
            match cast.as_ref().unwrap().latest() {
                Ok(Some(f)) => {
                    last_sent = Instant::now();
                    last = f.jpeg.clone();
                    last_dims = (f.width, f.height);
                    send(f.width, f.height, f.jpeg);
                }
                // A still screen produces no new frames: repeat the last one for late joiners.
                Ok(None) => {
                    if !last.is_empty() && last_sent.elapsed() > Duration::from_secs(1) {
                        last_sent = Instant::now();
                        send(last_dims.0, last_dims.1, last.clone());
                    }
                }
                Err(e) => {
                    cast = None;
                    fail(&e);
                }
            }
            std::thread::sleep(Duration::from_millis(60).saturating_sub(t0.elapsed()));
            continue;
        }

        if cap.is_none() {
            match capture::Capturer::new() {
                Ok(c) => cap = Some(c),
                Err(e) => {
                    fail(&e);
                    continue;
                }
            }
        }
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
                    send(width, height, jpeg);
                }
            }
            Err(e) => {
                // Stop presenting instead of leaving students on a black screen.
                fail(&e);
                cap = None;
            }
        }
        std::thread::sleep(Duration::from_millis(100).saturating_sub(t0.elapsed()));
    }
}

/// "aabbccddeeff" (or with `:` / `-`) → six bytes.
pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 || hex == "000000000000" {
        return None;
    }
    let mut out = [0u8; 6];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Wake-on-LAN "magic packet": 6 × 0xFF, then the address 16 times.
pub fn magic_packet(mac: [u8; 6]) -> Vec<u8> {
    let mut p = vec![0xFFu8; 6];
    for _ in 0..16 {
        p.extend_from_slice(&mac);
    }
    p
}

/// Sends the magic packet to the LAN broadcast address (UDP ports 9 and 7).
pub fn wake_on_lan(mac: [u8; 6]) -> std::io::Result<()> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0")?;
    sock.set_broadcast(true)?;
    let packet = magic_packet(mac);
    let mut last = Ok(());
    for port in [9u16, 7] {
        if let Err(e) = sock.send_to(&packet, ("255.255.255.255", port)) {
            last = Err(e);
        }
    }
    last
}

#[cfg(test)]
mod wol_tests {
    use super::*;

    #[test]
    fn mac_and_packet() {
        assert_eq!(
            parse_mac("aa:bb:cc:dd:ee:ff"),
            Some([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])
        );
        assert_eq!(parse_mac("AA-BB-CC-DD-EE-FF"), parse_mac("aabbccddeeff"));
        assert_eq!(parse_mac("000000000000"), None);
        assert_eq!(parse_mac("abc"), None);
        let p = magic_packet([1, 2, 3, 4, 5, 6]);
        assert_eq!(p.len(), 102);
        assert_eq!(&p[..6], &[0xFF; 6]);
        assert_eq!(&p[96..], &[1, 2, 3, 4, 5, 6]);
    }
}
