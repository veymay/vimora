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
    /// Connection key from the agent config. Empty in installed mode: the service holds it.
    key: Arc<String>,
    /// Client nonce + last privileged counter of the current session (local verification).
    session: Arc<Mutex<(Vec<u8>, u64)>>,
    /// Sender of the current session (None while disconnected).
    uplink: Arc<Mutex<Option<mpsc::Sender<ClientMsg>>>>,
    /// The docked chat panel, while a chat is open.
    chat: SharedChat,
}

pub async fn run(cfg: Config) -> Result<()> {
    info!(
        "Vimora agent starting (room: {:?})",
        if cfg.room.is_empty() { "*" } else { &cfg.room }
    );
    let state: Shared = Arc::default();
    let ctx = Ctx {
        state: state.clone(),
        engine: Engine::new(),
        key: Arc::new(cfg.key.clone()),
        session: Arc::default(),
        uplink: Arc::default(),
        chat: Arc::default(),
    };
    let (ov_tx, ov_rx) = mpsc::channel(16);
    tokio::spawn(overlay_task(state.clone(), ov_rx));
    tokio::spawn(blocker_task(state.clone()));
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
        let addr = match &cfg.server {
            Some(s) => s.clone(),
            None => discover(&cfg.room).await,
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
async fn discover(room: &str) -> String {
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
            if b.version != PROTOCOL_VERSION {
                continue;
            }
            if room.is_empty() || b.room.eq_ignore_ascii_case(room) {
                info!(
                    "found console '{}' (room '{}') at {}",
                    b.server_name,
                    b.room,
                    src.ip()
                );
                return format!("{}:{}", src.ip(), b.tcp_port);
            }
        }
    }
}
