//! The privileged Vimora service (`vimora-client service`), running as SYSTEM / root.
//!
//! It **pins** the public key of the first console the PC pairs with (in a file only
//! administrators can write) and executes privileged commands only when they carry that
//! console's signature for the current session. Talks to the user-session agent over
//! `127.0.0.1:47802`. `vimora-client unpair` (admin) forgets the console.

use crate::privileged::{Engine, PrivStatus};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};
use vimora_core::{
    auth::{self, ConsoleKey},
    net::{read_msg, write_msg},
    PrivCommand,
};

pub const DEFAULT_PORT: u16 = 47802;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcReq {
    /// Which console this PC is paired with (None = not paired yet).
    Pin,
    /// Check a console's session signature; pairs with it if the PC isn't paired yet.
    VerifyServer {
        console_key: ConsoleKey,
        client_nonce: Vec<u8>,
        server_nonce: Vec<u8>,
        sig: Vec<u8>,
    },
    Privileged {
        /// Which verified console session the command belongs to.
        client_nonce: Vec<u8>,
        counter: u64,
        payload: Vec<u8>,
        sig: Vec<u8>,
    },
    Status,
    // Append-only: an older service must keep understanding the requests it knows.
    /// Shadow overlay content found by the last list / commit.
    Changes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResp {
    Pin(Option<ConsoleKey>),
    Verified(bool),
    Done { ok: bool, message: String },
    Status(PrivStatus),
    Changes(Option<Vec<vimora_core::ChangeInfo>>),
}

/// The service's persistent state: the console this PC obeys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceConfig {
    /// Hex public key of the paired console.
    pub console_key: Option<String>,
}

impl ServiceConfig {
    fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<()> {
        let path = config_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        restrict(&path)
    }

    fn pinned(&self) -> Option<ConsoleKey> {
        self.console_key.as_deref().and_then(auth::key_from_hex)
    }
}

/// Trust-on-first-use pairing, persisted by the service.
struct Pairing(Mutex<ServiceConfig>);

impl Pairing {
    fn get(&self) -> Option<ConsoleKey> {
        self.0.lock().unwrap().pinned()
    }

    /// True if `key` is (now) the paired console.
    fn accept(&self, key: &ConsoleKey) -> bool {
        let mut c = self.0.lock().unwrap();
        match c.pinned() {
            Some(k) => k == *key,
            None => {
                c.console_key = Some(auth::to_hex(key));
                if let Err(e) = c.save() {
                    warn!("cannot save pairing: {e:#}");
                }
                info!("paired with console {}", auth::fingerprint(key));
                true
            }
        }
    }
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
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(serve(Pairing(Mutex::new(ServiceConfig::load()))))
}

/// `vimora-client unpair`: forget the console (the next one found is paired).
pub fn unpair() -> Result<()> {
    ServiceConfig::default().save()?;
    let _ = crate::config::forget_pin();
    println!("Konsol bilan bog'lanish o'chirildi. Keyingi topilgan konsol bilan qayta bog'lanadi.");
    Ok(())
}

async fn serve(pairing: Pairing) -> Result<()> {
    // Bind first: the watchdog starts a second copy every few minutes, and it must exit before
    // touching anything (resuming a commit cycle, updating…).
    let listener = TcpListener::bind(("127.0.0.1", port())).await?;
    info!("Vimora service listening on 127.0.0.1:{}", port());
    cleanup_old_binaries();
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
    {
        // Self-update from GitHub: shortly after start, then every 6 hours.
        let e = engine.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(180));
            loop {
                e.auto_update();
                std::thread::sleep(Duration::from_secs(6 * 3600));
            }
        });
    }
    let session: Arc<Mutex<Sessions>> = Arc::default();
    let pairing = Arc::new(pairing);
    loop {
        let (mut s, _) = listener.accept().await?;
        let (pairing, engine, session) = (pairing.clone(), engine.clone(), session.clone());
        tokio::spawn(async move {
            let Ok(req) = read_msg::<_, IpcReq>(&mut s).await else {
                return;
            };
            let resp = handle(req, &pairing, &engine, &session).await;
            let _ = write_msg(&mut s, &resp).await;
        });
    }
}

/// The Windows installer renames running executables to `*.old.exe` before replacing them.
fn cleanup_old_binaries() {
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        for n in ["vimora-client.old.exe", "vimora-server.old.exe"] {
            let _ = std::fs::remove_file(dir.join(n));
        }
    }
}

async fn handle(
    req: IpcReq,
    pairing: &Pairing,
    engine: &Arc<Engine>,
    session: &Mutex<Sessions>,
) -> IpcResp {
    match req {
        IpcReq::Pin => IpcResp::Pin(pairing.get()),
        IpcReq::VerifyServer {
            console_key,
            client_nonce,
            server_nonce,
            sig,
        } => {
            // Signature first, so a forged hello can't pair the PC with a fake console.
            let ok = auth::verify_welcome(&console_key, &client_nonce, &server_nonce, &sig)
                && pairing.accept(&console_key);
            if ok {
                session.lock().unwrap().add(client_nonce);
            }
            IpcResp::Verified(ok)
        }
        IpcReq::Privileged {
            client_nonce,
            counter,
            payload,
            sig,
        } => {
            let Some(console) = pairing.get() else {
                return IpcResp::Done {
                    ok: false,
                    message: "konsol bilan bog'lanmagan".into(),
                };
            };
            let cmd = {
                let mut s = session.lock().unwrap();
                let Some((last_counter, _)) = s.map.get_mut(&client_nonce) else {
                    return IpcResp::Done {
                        ok: false,
                        message: "konsol sessiyasi yo'q".into(),
                    };
                };
                if counter <= *last_counter
                    || !auth::verify_priv(&console, &client_nonce, counter, &payload, &sig)
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
        IpcReq::Changes => IpcResp::Changes(engine.changes()),
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

pub fn setup(server: Option<String>, name: Option<String>) -> Result<()> {
    // Service state (pairing), writable only by SYSTEM/Administrators (root).
    let path = config_path();
    if !path.exists() {
        ServiceConfig::default().save()?;
    }
    restrict(&path)?;
    // Re-running setup (every update does) keeps what an earlier install configured.
    let cfg = crate::config::load().merge_cli(server, name);
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
    // Restart if it ever stops: re-run every minute as a watchdog (a second instance exits on bind).
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
            "1",
            "/ru",
            "SYSTEM",
            "/rl",
            "HIGHEST",
            "/f",
        ],
    );
    // schtasks' defaults would stop the service on battery power (laptops!), never start it
    // there, and kill it after 3 days. Lift all of that.
    let (ok, out) = crate::sh::script(
        "$s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries \
         -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -MultipleInstances IgnoreNew \
         -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1); \
         foreach ($n in 'Vimora Service','Vimora Service Watchdog') { \
           Set-ScheduledTask -TaskName $n -Settings $s | Out-Null }",
    )
    .unwrap_or((false, String::new()));
    if !ok {
        tracing::warn!("task settings: {}", crate::sh::tail(&out, 2));
    }
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
    crate::sh::run_ok("systemctl", &["enable", "vimora-service"])?;
    // `restart`, not `start`: after an update the old service must make way for the new one.
    crate::sh::run_ok("systemctl", &["restart", "vimora-service"])?;
    println!("vimora-service.service yoqildi");
    Ok(())
}
