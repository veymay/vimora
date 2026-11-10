//! Headless agent: discovery, authenticated session with the console, command execution
//! and supervision of the overlay process.

mod chat_supervisor;
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
    ClientInfo, ClientMsg, ClientStatus, Command, FreezeState, NetPolicy, PowerAction, PrivCommand,
    ServerMsg, StreamConfig, DISCOVERY_PORT, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
};

#[derive(Default)]
struct State {
    lock: Option<(String, String)>,
    /// Offline unlock code of the current lock: (salt, hash, digits).
    unlock: Option<(Vec<u8>, Vec<u8>, u8)>,
    /// Icon for the lock screen (empty = default).
    lock_icon: String,
    show: bool,
    apps: Vec<String>,
    net: NetPolicy,
    help: bool,
    connected: bool,
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
    fn status(&self) -> ClientStatus {
        ClientStatus {
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

/// Things that outlive a single console session.
#[derive(Clone)]
struct Ctx {
    state: Shared,
    /// Local executor, used only when no Vimora service is running (dev / unprivileged mode).
    engine: Arc<Engine>,
    /// The console this PC follows (public key), once paired.
    pin: Arc<Mutex<Option<vimora_core::auth::ConsoleKey>>>,
    /// Client nonce + last privileged counter of the current session (local verification).
    session: Arc<Mutex<(Vec<u8>, u64)>>,
    /// Sender of the current session (None while disconnected).
    uplink: Arc<Mutex<Option<mpsc::Sender<ClientMsg>>>>,
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

/// Set when the console removed this PC (`PrivCommand::Unpair`): the session ends and that
/// console is left alone for a while.
pub(crate) static UNPAIRED: Mutex<bool> = Mutex::new(false);

/// Things the teacher should hear about (a window that cannot open…); sent as alerts by
/// `service_status_task` once a console is connected.
pub(crate) static FAULTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn report_fault(text: impl Into<String>) {
    let text = text.into();
    warn!("{text}");
    let mut f = FAULTS.lock().unwrap();
    if f.len() < 8 && !f.contains(&text) {
        f.push(text);
    }
}

/// The other graphics backend (see `crate::ui_renderer`).
pub(crate) fn switch_renderer() -> &'static str {
    let next = match std::env::var("VIMORA_RENDERER").as_deref() {
        Ok("wgpu") => "glow",
        _ => "wgpu",
    };
    std::env::set_var("VIMORA_RENDERER", next);
    next
}

/// Finds a graphics backend that can actually draw on this PC before any window is needed:
/// runs `vimora-client ui-probe` with each one and keeps the first that works.
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
    // Consoles that turned this PC away (or that it was just removed from): left alone for a
    // while, so another console on the network gets its turn.
    let mut ignored: std::collections::HashMap<String, Instant> = Default::default();
    loop {
        let pin = refresh_pin(&ctx).await;
        ignored.retain(|_, t| t.elapsed() < Duration::from_secs(90));
        let skip: Vec<String> = ignored.keys().cloned().collect();
        // Where to connect: a fixed address from the config, else the console that worked last
        // time (tried first), else whatever announces itself on the network.
        let mut targets: Vec<String> = vec![];
        match &cfg.server {
            Some(s) => targets.push(s.clone()),
            None => {
                if let Some(last) = crate::config::last_console().filter(|l| !skip.contains(l)) {
                    targets.push(last);
                }
                if let Ok(found) = tokio::time::timeout(
                    Duration::from_secs(if targets.is_empty() { 3600 } else { 6 }),
                    discover(pin, skip.clone()),
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
            info!("connecting to console {addr}");
            // The session runs as its own task, so even a panic in it only ends that session.
            let (c, a, x, o) = (cfg.clone(), addr.clone(), ctx.clone(), ov_tx.clone());
            let started = Instant::now();
            match tokio::spawn(async move { session(&c, &a, &x, &o).await }).await {
                Ok(Ok(())) => info!("console closed the connection"),
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
        // Retry quickly after a real session (the console may just have restarted), a little
        // slower when nothing could be reached.
        failures = if connected { 0 } else { (failures + 1).min(5) };
        tokio::time::sleep(Duration::from_secs(2 + failures as u64)).await;
    }
}

/// Wait for a console beacon matching our room; returns "ip:port".
/// The console this PC follows: the service's pairing is authoritative (it can't be edited by
/// the student); without a service the agent's own copy is used.
async fn refresh_pin(ctx: &Ctx) -> Option<vimora_core::auth::ConsoleKey> {
    let pin = match service::call(IpcReq::Pin).await {
        Ok(IpcResp::Pin(p)) => p,
        _ => crate::config::pin(),
    };
    *ctx.pin.lock().unwrap() = pin;
    pin
}

/// Waits for a console beacon; once paired, only the paired console's beacon counts.
async fn discover(pin: Option<vimora_core::auth::ConsoleKey>, skip: Vec<String>) -> String {
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
            let Some(b) = vimora_core::net::decode_beacon(&buf[..n]) else {
                continue;
            };
            if b.version < MIN_PROTOCOL_VERSION || pin.is_some_and(|k| k != b.console_key) {
                continue;
            }
            if skip.contains(&format!("{}:{}", src.ip(), b.tcp_port)) {
                continue;
            }
            info!(
                "found console '{}' at {} ({})",
                b.server_name,
                src.ip(),
                vimora_core::auth::fingerprint(&b.console_key)
            );
            return format!("{}:{}", src.ip(), b.tcp_port);
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
    let Some(start) = stamp(&exe) else { return };
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        match stamp(&exe) {
            Some(t) if t != start => {
                // Let the installer finish writing the file.
                tokio::time::sleep(Duration::from_secs(10)).await;
                let args: Vec<String> = std::env::args().skip(1).collect();
                match std::process::Command::new(&exe).args(&args).spawn() {
                    Ok(_) => {
                        info!("Vimora was updated — restarting");
                        std::process::exit(0);
                    }
                    Err(e) => warn!("cannot start the updated agent: {e}"),
                }
            }
            _ => {}
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
