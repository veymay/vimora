//! Console self-update: checks GitHub releases in the background, downloads and installs on request.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use vimora_update::{Release, Role};

#[derive(Clone, Default)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        version: String,
        done: u64,
        total: u64,
        /// Bytes per second, averaged since the start.
        speed: u64,
    },
    /// Checking the downloaded file against SHA256SUMS.
    Verifying(String),
    Installing(String),
    /// Ubuntu: the new version is installed; restart to use it.
    Installed(String),
    Failed(String),
}

#[derive(Default)]
struct State {
    phase: Phase,
    /// Windows: the installer is running, the console must close now.
    exit: bool,
    ctx: Option<eframe::egui::Context>,
}

#[derive(Clone, Default)]
pub struct Updater(Arc<Mutex<State>>);

impl Updater {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_ctx(&self, ctx: eframe::egui::Context) {
        self.0.lock().unwrap().ctx = Some(ctx);
    }

    pub fn phase(&self) -> Phase {
        self.0.lock().unwrap().phase.clone()
    }

    /// A newer version is waiting to be installed.
    pub fn available(&self) -> Option<String> {
        match &self.0.lock().unwrap().phase {
            Phase::Available(r) => Some(r.version.clone()),
            Phase::Installed(v) => Some(v.clone()),
            _ => None,
        }
    }

    /// Download / verify / install in progress (shown in the status bar too).
    pub fn working(&self) -> bool {
        matches!(
            self.0.lock().unwrap().phase,
            Phase::Downloading { .. } | Phase::Verifying(_) | Phase::Installing(_)
        )
    }

    pub fn take_exit(&self) -> bool {
        std::mem::take(&mut self.0.lock().unwrap().exit)
    }

    fn set(&self, phase: Phase) {
        let mut s = self.0.lock().unwrap();
        s.phase = phase;
        if let Some(c) = &s.ctx {
            c.request_repaint();
        }
    }

    fn busy(&self) -> bool {
        matches!(
            self.0.lock().unwrap().phase,
            Phase::Checking
                | Phase::Downloading { .. }
                | Phase::Verifying(_)
                | Phase::Installing(_)
        )
    }

    /// Shortly after start, then every 6 hours.
    pub fn start_background_checks(&self) {
        let me = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            loop {
                if !me.busy() && !matches!(me.phase(), Phase::Installed(_)) {
                    me.check_now();
                }
                std::thread::sleep(Duration::from_secs(6 * 3600));
            }
        });
    }

    /// Asks GitHub for the latest release (blocking; call from a thread or use [`check`]).
    fn check_now(&self) {
        self.set(Phase::Checking);
        match vimora_update::latest() {
            Ok(r) if r.is_newer() && r.asset(Role::Teacher).is_some() => {
                tracing::info!("update available: {}", r.version);
                self.set(Phase::Available(r));
            }
            Ok(_) => self.set(Phase::UpToDate),
            Err(e) => self.set(Phase::Failed(format!("{e:#}"))),
        }
    }

    pub fn check(&self) {
        if self.busy() {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || me.check_now());
    }

    /// Downloads and installs the available release.
    pub fn install(&self) {
        let Phase::Available(rel) = self.phase() else {
            return;
        };
        let me = self.clone();
        std::thread::spawn(move || {
            let v = rel.version.clone();
            let dir = std::env::temp_dir().join("vimora-update");
            let started = std::time::Instant::now();
            let last = Mutex::new(std::time::Instant::now() - Duration::from_secs(1));
            me.set(Phase::Downloading {
                version: v.clone(),
                done: 0,
                total: rel.asset(Role::Teacher).map_or(0, |a| a.size),
                speed: 0,
            });
            let progress = |done: u64, total: u64| {
                if total > 0 && done >= total {
                    me.set(Phase::Verifying(v.clone()));
                    return;
                }
                // ~10 updates a second is plenty for the bar.
                let mut l = last.lock().unwrap();
                if l.elapsed() < Duration::from_millis(100) {
                    return;
                }
                *l = std::time::Instant::now();
                let secs = started.elapsed().as_secs_f64().max(0.1);
                me.set(Phase::Downloading {
                    version: v.clone(),
                    done,
                    total,
                    speed: (done as f64 / secs) as u64,
                })
            };
            let res = vimora_update::download(&rel, Role::Teacher, &dir, &progress).and_then(|p| {
                me.set(Phase::Installing(v.clone()));
                vimora_update::install_console(&p)
            });
            match res {
                Ok(()) if cfg!(windows) => {
                    // The installer replaces the running console: close now.
                    let mut s = me.0.lock().unwrap();
                    s.exit = true;
                    if let Some(c) = &s.ctx {
                        c.request_repaint();
                    }
                }
                Ok(()) => me.set(Phase::Installed(v)),
                Err(e) => me.set(Phase::Failed(format!("{e:#}"))),
            }
        });
    }
}

/// Starts the freshly installed console and returns (the caller exits).
pub fn relaunch() {
    let installed = std::path::Path::new("/opt/vimora/bin/vimora-server");
    let exe = if installed.exists() {
        installed.to_path_buf()
    } else {
        match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => return,
        }
    };
    // Give this process a moment to exit and free the console's network ports.
    let r = std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 1; exec \"$0\"")
        .arg(exe)
        .spawn();
    if let Err(e) = r {
        tracing::warn!("cannot restart the console: {e}");
    }
}
