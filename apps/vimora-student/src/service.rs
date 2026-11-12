//! The privileged Vimora service (`vimora-student service`), running as SYSTEM / root.
//!
//! It **pins** the public key of the first teacher the PC pairs with (in a file only
//! administrators can write) and executes privileged commands only when they carry that
//! teacher's signature for the current session. Talks to the user-session agent over
//! `127.0.0.1:47802`. `vimora-student unpair` (admin) forgets the teacher.

use crate::privileged::{Engine, PrivStatus};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};
use vimora_core::{
    auth::{self, TeacherKey},
    net::{read_msg, write_msg},
    PrivCommand,
};

pub const DEFAULT_PORT: u16 = 47802;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcReq {
    /// Which teacher this PC is paired with (None = not paired yet).
    Pin,
    /// Check a teacher's session signature; pairs with it if the PC isn't paired yet.
    VerifyServer {
        teacher_key: TeacherKey,
        student_nonce: Vec<u8>,
        teacher_nonce: Vec<u8>,
        sig: Vec<u8>,
    },
    Privileged {
        /// Which verified teacher session the command belongs to.
        student_nonce: Vec<u8>,
        counter: u64,
        payload: Vec<u8>,
        sig: Vec<u8>,
    },
    Status,
    // Append-only: an older service must keep understanding the requests it knows.
    /// Shadow overlay content found by the last list / commit.
    Changes,
    /// The lock screen is up (`true`) or gone: switch the system's ways out off / back on.
    Kiosk(bool),
    /// An installer the teacher sent over the network (saved by the agent at `path`) with the
    /// signed list that vouches for it. The service checks the signature itself.
    InstallUpdate {
        path: String,
        sums: String,
        sig: String,
    },
    /// The tray window's "check for updates": installs a newer signed release now.
    UpdateNow,
    /// The kept list of pending system updates (and its revision).
    UpdatesInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResp {
    Pin(Option<TeacherKey>),
    Verified(bool),
    Done { ok: bool, message: String },
    Status(PrivStatus),
    Changes(Option<Vec<vimora_core::ChangeInfo>>),
    // Append-only.
    UpdatesInfo(Option<(u64, vimora_core::UpdatesInfo)>),
}

/// The service's persistent state: the teacher this PC obeys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceConfig {
    /// Hex public key of the paired teacher program (saved as "console_key" by 0.3.27 and
    /// older: still read).
    #[serde(alias = "console_key")]
    pub teacher_key: Option<String>,
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

    fn pinned(&self) -> Option<TeacherKey> {
        self.teacher_key.as_deref().and_then(auth::key_from_hex)
    }
}

/// Trust-on-first-use pairing, persisted by the service.
struct Pairing(Mutex<ServiceConfig>);

impl Pairing {
    /// Forgets the teacher: the next one found is paired.
    fn clear(&self) {
        let mut c = self.0.lock().unwrap();
        c.teacher_key = None;
        if let Err(e) = c.save() {
            warn!("cannot save pairing: {e:#}");
        }
    }

    fn get(&self) -> Option<TeacherKey> {
        self.0.lock().unwrap().pinned()
    }

    /// True if `key` is (now) the paired teacher.
    fn accept(&self, key: &TeacherKey) -> bool {
        let mut c = self.0.lock().unwrap();
        match c.pinned() {
            Some(k) => k == *key,
            None => {
                c.teacher_key = Some(auth::to_hex(key));
                if let Err(e) = c.save() {
                    warn!("cannot save pairing: {e:#}");
                }
                info!("paired with teacher {}", auth::fingerprint(key));
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
    let long = matches!(
        req,
        IpcReq::Privileged { .. } | IpcReq::InstallUpdate { .. } | IpcReq::UpdateNow
    );
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

/// Verified teacher sessions (student nonce → last accepted counter). Several agents can share
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

/// `vimora-student unpair`: forget the teacher (the next one found is paired).
pub fn unpair() -> Result<()> {
    ServiceConfig::default().save()?;
    let _ = crate::config::forget_pin();
    println!("O'qituvchi bilan bog'lanish o'chirildi. Keyingi topilgan o'qituvchi bilan qayta bog'lanadi.");
    Ok(())
}

async fn serve(pairing: Pairing) -> Result<()> {
    // Bind first: the watchdog starts a second copy every few minutes, and it must exit before
    // touching anything (resuming a commit cycle, updating…).
    let listener = TcpListener::bind(("127.0.0.1", port())).await?;
    info!("Vimora service listening on 127.0.0.1:{}", port());
    cleanup_old_binaries();
    exit_when_replaced();
    // An uninstall that waited for the freeze to be switched off.
    std::thread::spawn(crate::purge::finish_pending);
    // The agent is started again in the user's session if the student ends it somehow.
    crate::guard::keep_agent_running();
    std::thread::spawn(ensure_updater_tasks);
    crate::power::keep_awake();
    // A lock left over from before a crash or restart: release it; a still-locked agent
    // asks again within seconds.
    std::thread::spawn(|| {
        if let Err(e) = crate::kiosk::set(false) {
            tracing::warn!("kiosk release: {e:#}");
        }
    });
    let engine = Engine::new();
    // Updates and applications looked up in advance: the teacher's lists come at once.
    engine.prefetch();
    // A computer still in the old folder moves to the new one (only while not frozen).
    {
        let e = engine.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            crate::layout::migrate(e.shield().protected_now());
        });
    }
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
        // Self-update from GitHub: a minute after start, then every 30 minutes (a PC that
        // was asleep or offline catches up as soon as it can).
        let e = engine.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(60));
            loop {
                e.auto_update();
                std::thread::sleep(Duration::from_secs(30 * 60));
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
        for n in ["vimora-student.old.exe", "vimora-teacher.old.exe"] {
            let _ = std::fs::remove_file(dir.join(n));
        }
        // Files earlier updates moved aside ("vimora-student.old-<n>.exe"); those still running
        // can't be deleted and are left for later.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("vimora-") && n.contains(".old-") && n.ends_with(".exe") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

/// The update check on its own schedule (`vimora-student update-now` as SYSTEM, apart from the
/// service) must always be there: written again at every start (a frozen PC may have rolled it
/// back, an older version may have pointed it elsewhere).
fn ensure_updater_tasks() {
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        // The PowerShell updater of 0.3.22–0.3.26 is replaced by the program itself.
        if let Some(dir) = exe.parent() {
            let _ = std::fs::remove_file(dir.join("vimora-update.ps1"));
        }
        let tr = format!("\"{}\" update-now", exe.display());
        for (name, schedule) in [
            ("Vimora Updater", &["/sc", "hourly", "/mo", "4"][..]),
            (
                "Vimora Updater Boot",
                &["/sc", "onstart", "/delay", "0005:00"][..],
            ),
        ] {
            // Always written again: points at the current program, whatever an older version
            // registered there.
            let mut args = vec!["/create", "/tn", name, "/tr", &tr];
            args.extend_from_slice(schedule);
            args.extend_from_slice(&["/ru", "SYSTEM", "/rl", "HIGHEST", "/f"]);
            if let Err(e) = crate::sh::run_ok("schtasks", &args) {
                tracing::warn!("updater task {name}: {e:#}");
            }
        }
    }
}

/// An update installed a new version while this service kept running the old one (the
/// installer moves running files aside): start the new one and exit.
fn exit_when_replaced() {
    crate::selfcheck::watch("service", |_exe| {
        #[cfg(windows)]
        {
            // Exit first is not possible here (we'd be gone): ask the task to start the new
            // copy shortly after we left (it can't bind while we still hold the port).
            let _ = std::process::Command::new("cmd.exe")
                .args([
                    "/c",
                    "timeout /t 3 /nobreak >nul & schtasks /run /tn \"Vimora Service\"",
                ])
                .spawn();
        }
        std::process::exit(0);
    });
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
            teacher_key,
            student_nonce,
            teacher_nonce,
            sig,
        } => {
            // Signature first, so a forged hello can't pair the PC with a fake teacher.
            let was = pairing.get();
            let ok = auth::verify_welcome(&teacher_key, &student_nonce, &teacher_nonce, &sig)
                && pairing.accept(&teacher_key);
            if ok && was.is_none() {
                // Newly paired: keep it on a frozen PC too.
                engine.keep_files(&[config_path()]);
            }
            if ok {
                session.lock().unwrap().add(student_nonce);
            }
            IpcResp::Verified(ok)
        }
        IpcReq::Privileged {
            student_nonce,
            counter,
            payload,
            sig,
        } => {
            let Some(teacher) = pairing.get() else {
                return IpcResp::Done {
                    ok: false,
                    message: "o'qituvchi bilan bog'lanmagan".into(),
                };
            };
            let cmd = {
                let mut s = session.lock().unwrap();
                let Some((last_counter, _)) = s.map.get_mut(&student_nonce) else {
                    return IpcResp::Done {
                        ok: false,
                        message: "o'qituvchi sessiyasi yo'q".into(),
                    };
                };
                if counter <= *last_counter
                    || !auth::verify_priv(&teacher, &student_nonce, counter, &payload, &sig)
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
            if cmd == PrivCommand::Unpair {
                pairing.clear();
                // Released for good, also on a frozen PC.
                engine.keep_files(&[config_path()]);
                session.lock().unwrap().map.clear();
                return IpcResp::Done {
                    ok: true,
                    message: "Kompyuter o'qituvchidan ajratildi".into(),
                };
            }
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
        IpcReq::Kiosk(on) => {
            match tokio::task::spawn_blocking(move || crate::kiosk::set(on)).await {
                Ok(Ok(())) => IpcResp::Done {
                    ok: true,
                    message: String::new(),
                },
                Ok(Err(e)) => IpcResp::Done {
                    ok: false,
                    message: format!("{e:#}"),
                },
                Err(e) => IpcResp::Done {
                    ok: false,
                    message: e.to_string(),
                },
            }
        }
        IpcReq::InstallUpdate { path, sums, sig } => {
            let e = engine.clone();
            let res = tokio::task::spawn_blocking(move || {
                e.install_pushed(std::path::Path::new(&path), &sums, &sig)
            })
            .await;
            match res {
                Ok(Ok(message)) => IpcResp::Done { ok: true, message },
                Ok(Err(e)) => IpcResp::Done {
                    ok: false,
                    message: format!("{e:#}"),
                },
                Err(e) => IpcResp::Done {
                    ok: false,
                    message: e.to_string(),
                },
            }
        }
        IpcReq::UpdatesInfo => IpcResp::UpdatesInfo(engine.updates_info()),
        IpcReq::UpdateNow => {
            let e = engine.clone();
            match tokio::task::spawn_blocking(move || e.update_from_student()).await {
                Ok(Ok(message)) => IpcResp::Done { ok: true, message },
                Ok(Err(e)) => IpcResp::Done {
                    ok: false,
                    message: format!("{e:#}"),
                },
                Err(e) => IpcResp::Done {
                    ok: false,
                    message: e.to_string(),
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
// `vimora-student setup`: writes configs and registers the service (run elevated by installers).

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
    // there, and kill it after 3 days: lift all of that. And the student program itself: started
    // by the scheduler the moment any user signs in (earlier than the Run key, which waits for
    // the desktop), restarted if it dies; two copies starting together settle it between
    // themselves (`single.rs`). One PowerShell run for both (each start of it takes seconds).
    let agent_exe = exe.replace('\'', "''");
    let (ok, out) = crate::sh::script(&format!(
        "$s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries \
         -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -MultipleInstances IgnoreNew \
         -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1); \
         foreach ($n in 'Vimora Service','Vimora Service Watchdog') {{ \
           Set-ScheduledTask -TaskName $n -Settings $s | Out-Null }}; \
         $a = New-ScheduledTaskAction -Execute '{agent_exe}'; \
         $t = New-ScheduledTaskTrigger -AtLogOn; \
         $p = New-ScheduledTaskPrincipal -GroupId 'BUILTIN\\Users' -RunLevel Limited; \
         Register-ScheduledTask -TaskName 'Vimora Student' -Action $a -Trigger $t -Principal $p \
           -Settings $s -Force | Out-Null"
    ))
    .unwrap_or((false, String::new()));
    if !ok {
        tracing::warn!("task settings: {}", crate::sh::tail(&out, 2));
    }
    register_rest(&exe);
    crate::sh::run_ok("schtasks", &["/run", "/tn", "Vimora Service"])?;
    println!("Vimora Service (SYSTEM) ro'yxatga olindi va ishga tushdi");
    Ok(())
}

/// The rest of a student install, done here (not by the installer) so that moving Vimora to
/// another folder (see `layout::migrate`) registers everything again the same way.
#[cfg(windows)]
fn register_rest(exe: &str) {
    let sh = |cmd: &str, args: &[&str]| {
        if let Err(e) = crate::sh::run_ok(cmd, args) {
            tracing::warn!(
                "setup: {cmd} {}: {e:#}",
                args.first().copied().unwrap_or("")
            );
        }
    };
    let quiet = |cmd: &str, args: &[&str]| {
        let _ = crate::sh::run(cmd, args);
    };
    let dir = std::path::Path::new(exe)
        .parent()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Names used up to 0.3.27.
    const RUN: &str = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    quiet("reg", &["delete", RUN, "/v", "VimoraAgent", "/f"]);
    quiet("schtasks", &["/delete", "/tn", "Vimora Agent", "/f"]);
    quiet(
        "netsh",
        &[
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            "name=Vimora Agent",
        ],
    );
    // Started in every user's session at sign-in (it needs the desktop).
    let quoted = format!("\"{exe}\"");
    sh(
        "reg",
        &[
            "add",
            RUN,
            "/v",
            "VimoraStudent",
            "/t",
            "REG_SZ",
            "/d",
            &quoted,
            "/f",
        ],
    );
    // Discovery beacons and "Skanerlash" from the teacher.
    quiet(
        "netsh",
        &[
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            "name=Vimora Student",
        ],
    );
    let program = format!("program={exe}");
    sh(
        "netsh",
        &[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=Vimora Student",
            "dir=in",
            "action=allow",
            "protocol=UDP",
            "localport=47800",
            &program,
        ],
    );
    ensure_updater_tasks();
    // Files the teacher sends land in Vimora's own folder; the student may open and save them.
    let files = format!("{dir}\\Files");
    let _ = std::fs::create_dir_all(&files);
    quiet("icacls", &[&files, "/grant", "*S-1-5-32-545:(OI)(CI)M"]);
    // The entry in the program list: hidden, and without "Uninstall" — only the teacher removes
    // Vimora from a student computer.
    const UN: &str = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Vimora";
    let uninstall = format!("\"{dir}\\uninstall.exe\"");
    let icon = format!("{dir}\\vimora.ico");
    for (name, value) in [
        ("DisplayName", "Vimora"),
        ("DisplayVersion", vimora_update::current()),
        ("Publisher", "VEYMAY"),
        ("DisplayIcon", icon.as_str()),
        ("InstallLocation", dir.as_str()),
        ("UninstallString", uninstall.as_str()),
        ("URLInfoAbout", "https://github.com/veymay/vimora"),
    ] {
        sh(
            "reg",
            &["add", UN, "/v", name, "/t", "REG_SZ", "/d", value, "/f"],
        );
    }
    for name in ["NoModify", "NoRepair", "NoRemove", "SystemComponent"] {
        sh(
            "reg",
            &["add", UN, "/v", name, "/t", "REG_DWORD", "/d", "1", "/f"],
        );
    }
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
