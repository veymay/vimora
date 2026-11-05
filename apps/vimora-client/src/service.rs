//! The privileged Vimora service (`vimora-client service`), running as SYSTEM / root.
//!
//! It holds the connection key (in a file only administrators can read), so the user-session
//! agent asks it to compute/verify handshake MACs, and it only executes privileged commands that
//! carry a valid console signature for the current session. Talks to the agent over
//! `127.0.0.1:47802`.

use crate::privileged::{Engine, PrivStatus};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};
use vimora_core::{
    auth,
    net::{read_msg, write_msg},
    PrivCommand,
};

pub const DEFAULT_PORT: u16 = 47802;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcReq {
    ClientMac {
        server_nonce: Vec<u8>,
        client_nonce: Vec<u8>,
    },
    VerifyServer {
        client_nonce: Vec<u8>,
        server_nonce: Vec<u8>,
        mac: Vec<u8>,
    },
    Privileged {
        /// Which verified console session the command belongs to.
        client_nonce: Vec<u8>,
        counter: u64,
        payload: Vec<u8>,
        mac: Vec<u8>,
    },
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResp {
    Mac(Vec<u8>),
    Verified(bool),
    Done { ok: bool, message: String },
    Status(PrivStatus),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub key: String,
}

pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("VIMORA_SERVICE_CONFIG") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        let pd = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
        PathBuf::from(pd).join("Vimora").join("service.json")
    }
    #[cfg(not(windows))]
    PathBuf::from("/etc/vimora/service.json")
}

pub fn port() -> u16 {
    std::env::var("VIMORA_SERVICE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Agent side: one request/response round-trip.
pub async fn call(req: IpcReq) -> Result<IpcResp> {
    let long = matches!(req, IpcReq::Privileged { .. });
    let mut s = tokio::time::timeout(
        Duration::from_secs(2),
        TcpStream::connect(("127.0.0.1", port())),
    )
    .await
    .context("Vimora xizmati javob bermadi")??;
    write_msg(&mut s, &req).await?;
    let wait = if long {
        Duration::from_secs(900)
    } else {
        Duration::from_secs(10)
    };
    tokio::time::timeout(wait, read_msg(&mut s))
        .await
        .context("Vimora xizmati javob bermadi")?
}

/// Verified console sessions (client nonce → last accepted counter). Several agents can share
/// one service (multiple users logged in, or test setups), so each keeps its own session.
#[derive(Default)]
struct Sessions {
    map: std::collections::HashMap<Vec<u8>, (u64, std::time::Instant)>,
}

impl Sessions {
    const MAX: usize = 64;

    fn add(&mut self, nonce: Vec<u8>) {
        if self.map.len() >= Self::MAX {
            // Drop the oldest session.
            if let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, (_, t))| *t)
                .map(|(k, _)| k.clone())
            {
                self.map.remove(&oldest);
            }
        }
        self.map.insert(nonce, (0, std::time::Instant::now()));
    }
}

pub fn run() -> Result<()> {
    let path = config_path();
    let cfg: ServiceConfig = serde_json::from_str(
        &std::fs::read_to_string(&path).with_context(|| format!("{} topilmadi", path.display()))?,
    )?;
    if cfg.key.is_empty() {
        bail!("{} da ulanish kaliti yo'q", path.display());
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(serve(cfg.key))
}

async fn serve(key: String) -> Result<()> {
    let engine = Engine::new();
    {
        let e = engine.clone();
        std::thread::spawn(move || e.resume());
    }
    {
        let e = engine.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(20)).await;
                let e = e.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    e.enforce_net();
                    e.reconcile();
                })
                .await;
            }
        });
    }
    let session: Arc<Mutex<Sessions>> = Arc::default();
    let listener = TcpListener::bind(("127.0.0.1", port())).await?;
    info!("Vimora service listening on 127.0.0.1:{}", port());
    let key = Arc::new(key);
    loop {
        let (mut s, _) = listener.accept().await?;
        let (key, engine, session) = (key.clone(), engine.clone(), session.clone());
        tokio::spawn(async move {
            let Ok(req) = read_msg::<_, IpcReq>(&mut s).await else {
                return;
            };
            let resp = handle(req, &key, &engine, &session).await;
            let _ = write_msg(&mut s, &resp).await;
        });
    }
}

async fn handle(
    req: IpcReq,
    key: &str,
    engine: &Arc<Engine>,
    session: &Mutex<Sessions>,
) -> IpcResp {
    match req {
        IpcReq::ClientMac {
            server_nonce,
            client_nonce,
        } => IpcResp::Mac(auth::client_mac(key, &server_nonce, &client_nonce)),
        IpcReq::VerifyServer {
            client_nonce,
            server_nonce,
            mac,
        } => {
            let ok = auth::verify_server(key, &client_nonce, &server_nonce, &mac);
            if ok {
                session.lock().unwrap().add(client_nonce);
            }
            IpcResp::Verified(ok)
        }
        IpcReq::Privileged {
            client_nonce,
            counter,
            payload,
            mac,
        } => {
            let cmd = {
                let mut s = session.lock().unwrap();
                let Some((last_counter, _)) = s.map.get_mut(&client_nonce) else {
                    return IpcResp::Done {
                        ok: false,
                        message: "konsol sessiyasi yo'q".into(),
                    };
                };
                if counter <= *last_counter
                    || !auth::verify_priv(key, &client_nonce, counter, &payload, &mac)
                {
                    warn!("rejected privileged command (bad signature or replay)");
                    return IpcResp::Done {
                        ok: false,
                        message: "imzo noto'g'ri — buyruq rad etildi".into(),
                    };
                }
                *last_counter = counter;
                match bincode::deserialize::<PrivCommand>(&payload) {
                    Ok(c) => c,
                    Err(e) => {
                        return IpcResp::Done {
                            ok: false,
                            message: format!("noto'g'ri buyruq: {e}"),
                        }
                    }
                }
            };
            info!("privileged: {cmd:?}");
            let e = engine.clone();
            match tokio::task::spawn_blocking(move || e.execute(cmd)).await {
                Ok(Ok(m)) => IpcResp::Done {
                    ok: true,
                    message: m,
                },
                Ok(Err(err)) => IpcResp::Done {
                    ok: false,
                    message: format!("{err:#}"),
                },
                Err(err) => IpcResp::Done {
                    ok: false,
                    message: err.to_string(),
                },
            }
        }
        IpcReq::Status => {
            let e = engine.clone();
            IpcResp::Status(
                tokio::task::spawn_blocking(move || e.status())
                    .await
                    .unwrap_or_default(),
            )
        }
    }
}

// ---------------------------------------------------------------------------------------------
// `vimora-client setup`: writes configs and registers the service (run elevated by installers).

pub fn setup(key: &str, server: Option<String>, name: Option<String>) -> Result<()> {
    if key.is_empty() {
        bail!("--key kerak");
    }
    // Service config: key, readable only by SYSTEM/Administrators (root).
    let path = config_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&ServiceConfig { key: key.into() })?,
    )?;
    restrict(&path)?;

    // Agent config without the key: the agent authenticates through the service.
    let cfg = crate::config::Config {
        key: String::new(),
        server,
        name,
        ..Default::default()
    };
    let cpath = crate::config::save(&cfg)?;
    println!("agent: {}\nservice: {}", cpath.display(), path.display());
    register()
}

#[cfg(windows)]
fn restrict(path: &std::path::Path) -> Result<()> {
    let p = path.to_string_lossy();
    crate::sh::run_ok(
        "icacls",
        &[
            &p,
            "/inheritance:r",
            "/grant:r",
            "*S-1-5-18:F",
            "*S-1-5-32-544:F",
        ],
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn restrict(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn register() -> Result<()> {
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let tr = format!("\"{exe}\" service");
    crate::sh::run_ok(
        "schtasks",
        &[
            "/create",
            "/tn",
            "Vimora Service",
            "/tr",
            &tr,
            "/sc",
            "onstart",
            "/ru",
            "SYSTEM",
            "/rl",
            "HIGHEST",
            "/f",
        ],
    )?;
    // Restart if it ever stops: re-run every 5 minutes as a watchdog (a second instance exits on bind).
    let _ = crate::sh::run(
        "schtasks",
        &[
            "/create",
            "/tn",
            "Vimora Service Watchdog",
            "/tr",
            &tr,
            "/sc",
            "minute",
            "/mo",
            "5",
            "/ru",
            "SYSTEM",
            "/rl",
            "HIGHEST",
            "/f",
        ],
    );
    crate::sh::run_ok("schtasks", &["/run", "/tn", "Vimora Service"])?;
    println!("Vimora Service (SYSTEM) ro'yxatga olindi va ishga tushdi");
    Ok(())
}

#[cfg(not(windows))]
fn register() -> Result<()> {
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let unit = format!(
        "[Unit]\nDescription=Vimora privileged service (freeze, updates, NetControl)\nAfter=network-online.target\n\n\
         [Service]\nExecStart={exe} service\nRestart=always\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\n"
    );
    std::fs::write("/etc/systemd/system/vimora-service.service", unit)?;
    crate::sh::run_ok("systemctl", &["daemon-reload"])?;
    crate::sh::run_ok("systemctl", &["enable", "--now", "vimora-service"])?;
    println!("vimora-service.service yoqildi");
    Ok(())
}
