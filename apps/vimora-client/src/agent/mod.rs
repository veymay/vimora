//! Headless agent: discovery, authenticated session with the console, command execution
//! and supervision of the overlay process.

mod chat_supervisor;
mod overlay_supervisor;
mod session;
mod tasks;

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
    ServerMsg, StreamConfig, DISCOVERY_PORT, PROTOCOL_VERSION,
};

#[derive(Default)]
struct State {
    lock: Option<(String, String)>,
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
    let (ov_tx, ov_rx) = mpsc::channel(16);
    tokio::spawn(overlay_task(state.clone(), ov_rx));
    tokio::spawn(blocker_task(state.clone()));
    tokio::spawn(restart_on_update());
    tokio::spawn(net_enforce_task(ctx.engine.clone()));
    tokio::spawn(service_status_task(ctx.clone()));
    {
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("keyword-watch".into())
            .spawn(move || keyword_watch(ctx))?;
    }
    tokio::spawn(auto_unlock_task(
        state.clone(),
        ov_tx.clone(),
        cfg.unlock_after_disconnect_secs,
    ));

    loop {
        let pin = refresh_pin(&ctx).await;
        let addr = match &cfg.server {
            Some(s) => s.clone(),
            None => discover(pin).await,
        };
        info!("connecting to console {addr}");
        match session(&cfg, &addr, &ctx, &ov_tx).await {
            Ok(()) => info!("console closed the connection"),
            Err(e) => warn!("session ended: {e:#}"),
        }
        *ctx.uplink.lock().unwrap() = None;
        {
            let mut s = state.lock().unwrap();
            s.connected = false;
            s.disconnected_at = Some(Instant::now());
            s.show = false;
        }
        let _ = ov_tx.send(OverlayReq::Sync).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
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
async fn discover(pin: Option<vimora_core::auth::ConsoleKey>) -> String {
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
            if b.version != PROTOCOL_VERSION || pin.is_some_and(|k| k != b.console_key) {
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
