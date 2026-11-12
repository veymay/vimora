//! Executes [`PrivCommand`]s: Shadow mode, updates, applications, renaming, power, NetControl.
//!
//! Lives inside the Vimora service (SYSTEM / root). The user-session agent only forwards the
//! teacher's signed commands to it, so students can't trigger these by themselves.
//!
//! **Teacher changes always persist.** While Shadow mode protects the PC, every setting this
//! engine changes is committed through the write filter, and work that can't be committed
//! piecemeal (installing updates, renaming) is queued for a commit cycle.

use crate::shield::{Shield, Task};
use crate::{apps, netcontrol::NetControl, sh, system, updates};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vimora_core::{AppInfo, ChangeInfo, FreezeState, PowerAction, PrivCommand, ShieldUsage};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrivStatus {
    pub freeze: FreezeState,
    pub shield_usage: Option<ShieldUsage>,
    pub shield_note: Option<String>,
    pub os_updates: Option<u32>,
    pub app_updates: Option<u32>,
    pub busy: Option<String>,
    pub queued: u32,
    pub persistent_folders: Vec<String>,
    /// Last application list and its revision (bumped on every change).
    pub apps: Option<(u64, Vec<AppInfo>)>,
    /// Results of background jobs since the last status call: (ok, message).
    pub events: Vec<(bool, String)>,
}

#[derive(Default)]
struct Inner {
    freeze: FreezeState,
    shield_usage: Option<ShieldUsage>,
    shield_note: Option<String>,
    freeze_checked: Option<Instant>,
    os_updates: Option<u32>,
    app_updates: Option<u32>,
    busy: Option<String>,
    apps: Option<(u64, Vec<AppInfo>)>,
    changes: Option<Vec<ChangeInfo>>,
    events: Vec<(bool, String)>,
}

#[derive(Default)]
pub struct Engine {
    net: Mutex<NetControl>,
    shield: Shield,
    inner: Mutex<Inner>,
}

impl Engine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn shield(&self) -> &Shield {
        &self.shield
    }

    fn push(&self, ok: bool, msg: String) {
        self.inner.lock().unwrap().events.push((ok, msg));
    }

    fn refresh_freeze(&self) {
        let p = self.shield.probe();
        let mut i = self.inner.lock().unwrap();
        i.freeze = p.state;
        i.shield_usage = p.usage;
        i.shield_note = p.note;
        i.freeze_checked = Some(Instant::now());
    }

    /// The overlay content found by the last `ListChanges` / `CommitPaths`.
    pub fn changes(&self) -> Option<Vec<ChangeInfo>> {
        self.inner.lock().unwrap().changes.clone()
    }

    /// Re-reads the overlay content.
    fn refresh_changes(&self) -> Result<String> {
        let list = self.shield.changes()?;
        let Some(list) = list else {
            bail!("bu tizimda overlay tarkibini ko'rib bo'lmaydi (Windows UWF) — yo'lni qo'lda kiriting");
        };
        let n = list.len();
        self.inner.lock().unwrap().changes = Some(list);
        Ok(format!("{n} ta o'zgarish topildi"))
    }

    /// Makes these files survive restarts if the PC is protected right now.
    pub fn keep_files(&self, files: &[std::path::PathBuf]) {
        let refs: Vec<&std::path::Path> = files.iter().map(|p| p.as_path()).collect();
        self.shield.commit_files(&refs);
    }

    /// Commits what an operation changed if the PC is protected right now.
    fn commit(&self, t: &sh::Touched) {
        let files: Vec<&std::path::Path> = t.files.iter().map(|p| p.as_path()).collect();
        self.shield.commit_files(&files);
        self.shield.commit_registry(&t.registry);
    }

    /// Self-healing and capacity checks; call periodically (service).
    pub fn reconcile(&self) {
        for (ok, msg) in self.shield.reconcile() {
            self.push(ok, msg);
        }
        self.flush_commits();
        self.refresh_freeze();
    }

    /// Background self-update (service, every 30 minutes) unless the teacher turned it off.
    pub fn auto_update(self: &Arc<Self>) {
        // An update started earlier that never arrived is reported (and retried below).
        self.check_pending_update();
        if self.shield.store().vimora_manual || self.inner.lock().unwrap().busy.is_some() {
            return;
        }
        // The same version failed several times: try it only every 6 hours.
        if let Some(p) = PendingUpdate::load() {
            if p.tries >= 3 && now_secs().saturating_sub(p.at) < 6 * 3600 {
                return;
            }
        }
        let res = self.update_vimora();
        // update_vimora shows its progress as "busy"; outside a job nobody else clears it.
        self.inner.lock().unwrap().busy = None;
        match res {
            Ok(m) if m.contains("o'rnatilmoqda") => self.push(true, m),
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{e:#}");
                tracing::warn!("self-update skipped: {msg}");
                // No internet is normal in a classroom; anything else the teacher should see.
                let offline = msg.contains("bog'lanib bo'lmadi") || msg.contains("reliz topilmadi");
                if !offline {
                    self.push(false, format!("Vimora yangilanmadi: {msg}"));
                }
            }
        }
    }

    /// After an installer was started: did the new version arrive? Told to the teacher once.
    fn check_pending_update(&self) {
        let Some(mut p) = PendingUpdate::load() else {
            return;
        };
        let current = vimora_update::current();
        if !vimora_update::is_newer(&p.version, current) {
            PendingUpdate::clear();
            self.push(true, format!("Vimora {current} o'rnatildi"));
            return;
        }
        // The installer finished with an error, or it is still the old version 10 minutes
        // later: it failed. Say why when the installer told us.
        let code = vimora_update::install_result(&update_dir()).filter(|c| *c != 0);
        if !p.reported && (code.is_some() || now_secs().saturating_sub(p.at) > 600) {
            p.reported = true;
            p.save();
            let why = match code {
                Some(c) => format!("o'rnatuvchi xato kodi {c}"),
                None => "o'rnatuvchi tugamadi".to_string(),
            };
            self.push(
                false,
                format!(
                    "Vimora {} o'rnatilmadi ({why}, {}-urinish) — keyinroq yana urinib ko'riladi",
                    p.version, p.tries
                ),
            );
        }
    }

    /// Installs the newest GitHub release if it is newer than this build.
    fn update_vimora(self: &Arc<Self>) -> Result<String> {
        use vimora_update::Role;
        let rel = vimora_update::latest()?;
        if !rel.is_newer() {
            return Ok(format!(
                "Vimora {} — eng so'nggi versiya",
                vimora_update::current()
            ));
        }
        if let Some(m) = install_running(&rel.version) {
            return Ok(m);
        }
        let _lock = UpdateLock::take()?;
        // Progress goes to the teacher through the status ("busy"), e.g.
        // "Vimora 0.3.9 yuklanmoqda 45%" — the teacher draws a bar from the percent.
        let v = rel.version.clone();
        let last = std::sync::Mutex::new(u64::MAX);
        let progress = |done: u64, total: u64| {
            if total == 0 {
                return;
            }
            let pct = (done * 100 / total).min(100);
            let mut l = last.lock().unwrap();
            if *l != pct {
                *l = pct;
                self.inner.lock().unwrap().busy = Some(format!("Vimora {v} yuklanmoqda {pct}%"));
            }
        };
        vimora_update::clear_old(&update_dir());
        let path = vimora_update::download(&rel, Role::Student, &update_dir(), &progress)?;
        self.start_install(&path, &rel.version)
    }

    /// The student's own "check for updates" (tray window): the same signed-only update, now.
    pub fn update_from_student(self: &Arc<Self>) -> Result<String> {
        if let Some(b) = self.inner.lock().unwrap().busy.clone() {
            return Ok(b);
        }
        let res = self.update_vimora();
        self.inner.lock().unwrap().busy = None;
        res
    }

    /// An installer the teacher sent over the network (for computers without internet): the
    /// signature of its list and its checksum are checked here, so a fake one is refused.
    pub fn install_pushed(
        self: &Arc<Self>,
        file: &std::path::Path,
        sums: &str,
        sig: &str,
    ) -> Result<String> {
        let res = self.install_pushed_inner(file, sums, sig);
        // Shown as "busy" while it runs; nobody else clears it.
        self.inner.lock().unwrap().busy = None;
        if let Ok(m) = &res {
            self.push(true, m.clone());
        }
        res
    }

    fn install_pushed_inner(
        self: &Arc<Self>,
        file: &std::path::Path,
        sums: &str,
        sig: &str,
    ) -> Result<String> {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .context("fayl nomi yo'q")?;
        let data = std::fs::read(file).context("yangilanish faylini o'qib bo'lmadi")?;
        let version = vimora_update::verify_package(&name, &data, sums, sig)?;
        if !vimora_update::is_newer(&version, vimora_update::current()) {
            return Ok(format!(
                "Vimora {} — eng so'nggi versiya",
                vimora_update::current()
            ));
        }
        if let Some(m) = install_running(&version) {
            return Ok(m);
        }
        let _lock = UpdateLock::take()?;
        vimora_update::clear_old(&update_dir());
        // Our own copy in the service's folder (the student cannot change it after the check).
        let path = vimora_update::save(&update_dir(), &name, &data)?;
        vimora_update::verify_package(&name, &std::fs::read(&path)?, sums, sig)?;
        self.start_install(&path, &version)
    }

    /// Starts a checked installer and remembers it (see `check_pending_update`).
    fn start_install(self: &Arc<Self>, path: &std::path::Path, version: &str) -> Result<String> {
        if self.shield.protected_now() {
            if !cfg!(windows) {
                bail!(
                    "Muzlatish yoqilgan: Vimora {version} ni o'rnatish uchun avval uni o'chiring"
                );
            }
            // The freeze would throw the new files away at the next restart (the PC would fall
            // back to the old version every time): the new service commits them once the
            // installer is done.
            if let Some(dir) = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
            {
                self.shield.queue_commit(vec![dir]);
            }
        }
        self.inner.lock().unwrap().busy = Some(format!("Vimora {version} o'rnatilmoqda"));
        let tries = PendingUpdate::load()
            .filter(|p| p.version == version)
            .map_or(0, |p| p.tries)
            + 1;
        PendingUpdate {
            version: version.to_string(),
            at: now_secs(),
            tries,
            reported: false,
        }
        .save();
        vimora_update::install_student(path)?;
        // The installer replaces this service when it works. When it fails, the teacher hears
        // it at once (not only at the next update check).
        let me = self.clone();
        std::thread::spawn(move || {
            for _ in 0..180 {
                std::thread::sleep(Duration::from_secs(5));
                if let Some(code) = vimora_update::install_result(&update_dir()) {
                    if code != 0 {
                        me.check_pending_update();
                    }
                    return;
                }
            }
            me.check_pending_update();
        });
        Ok(format!("Vimora {version} o'rnatilmoqda"))
    }

    /// Runs `f` on a background thread; the outcome arrives later through [`status`].
    fn job(
        self: &Arc<Self>,
        label: &str,
        f: impl FnOnce(&Arc<Engine>) -> Result<String> + Send + 'static,
    ) -> Result<String> {
        {
            let mut i = self.inner.lock().unwrap();
            if let Some(b) = &i.busy {
                bail!("hozir boshqa jarayon ketmoqda: {b}");
            }
            i.busy = Some(label.to_string());
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let res = f(&me);
            me.inner.lock().unwrap().busy = None;
            match res {
                Ok(m) => me.push(true, m),
                Err(e) => me.push(false, format!("{e:#}")),
            }
        });
        Ok(format!("{label}…"))
    }

    pub fn execute(self: &Arc<Self>, cmd: PrivCommand) -> Result<String> {
        let msg = match cmd {
            PrivCommand::Freeze { action, restart } => {
                let msg = self.shield.apply(action)?;
                self.refresh_freeze();
                if restart {
                    sh::restart_soon()?;
                    format!("{msg} — qayta yuklanmoqda")
                } else {
                    msg
                }
            }
            PrivCommand::SetUpdatePolicy(p) => {
                let msg = updates::set_policy(p)?;
                self.commit(&updates::policy_touched());
                self.shield.set_vimora_auto(p.vimora);
                msg
            }
            PrivCommand::UpdateVimora => {
                // Already updating by itself: not an error for the teacher.
                let running = self.inner.lock().unwrap().busy.clone();
                match running {
                    Some(b) if b.contains("Vimora") => format!("{b} (allaqachon boshlangan)"),
                    _ => self.job("Vimora yangilanmoqda", |e| e.update_vimora())?,
                }
            }
            PrivCommand::CheckUpdates => self.job("Yangilanishlar tekshirilmoqda", |e| {
                let p = updates::check()?;
                let mut i = e.inner.lock().unwrap();
                i.os_updates = Some(p.os);
                i.app_updates = Some(p.apps);
                Ok(format!(
                    "Yangilanishlar: tizim {} ta, dasturlar {} ta",
                    p.os, p.apps
                ))
            })?,
            PrivCommand::InstallUpdates { os, apps, restart } => {
                if self.shield.protected_now() {
                    // Installing on the overlay would vanish: commit cycle instead.
                    let msg = self.shield.queue(Task::InstallUpdates { os, apps })?;
                    self.refresh_freeze();
                    if restart {
                        sh::restart_soon()?;
                    }
                    return Ok(msg);
                }
                self.job("Yangilanishlar o'rnatilmoqda", move |e| {
                    let report = updates::install(os, apps, &e.shield.store().app_updates)?;
                    {
                        let mut i = e.inner.lock().unwrap();
                        if os {
                            i.os_updates = Some(0);
                        }
                        if apps {
                            i.app_updates = Some(0);
                        }
                    }
                    if restart && updates::reboot_required() {
                        sh::restart_soon()?;
                        return Ok(format!("{report} — qayta yuklanmoqda"));
                    }
                    Ok(report)
                })?
            }
            PrivCommand::Power(a) => {
                if a == PowerAction::Logoff {
                    bail!("seansdan chiqarish foydalanuvchi agentida bajariladi");
                }
                system::power(a)?;
                format!("{a:?}")
            }
            PrivCommand::SetNetPolicy(p) => {
                let mut net = self.net.lock().unwrap();
                let msg = net.apply(p)?;
                self.commit(&net.touched());
                msg
            }
            PrivCommand::ListApps => self.job("Dasturlar ro'yxati olinmoqda", |e| {
                let list = apps::list(&e.shield.store().app_updates)?;
                let n = list.len();
                let mut i = e.inner.lock().unwrap();
                let rev = i.apps.as_ref().map_or(1, |(r, _)| r + 1);
                i.apps = Some((rev, list));
                Ok(format!("{n} ta dastur topildi"))
            })?,
            PrivCommand::UninstallVimora => {
                remove_vimora()?;
                "Vimora o'chirilmoqda".into()
            }
            PrivCommand::UninstallApp(id) => self.job("Dastur o'chirilmoqda", move |e| {
                let msg = apps::uninstall(&id)?;
                // A fresh list goes to the teacher (the removed one is gone from it).
                if let Ok(list) = apps::list(&e.shield.store().app_updates) {
                    let mut i = e.inner.lock().unwrap();
                    let rev = i.apps.as_ref().map_or(1, |(r, _)| r + 1);
                    i.apps = Some((rev, list));
                }
                Ok(msg)
            })?,
            PrivCommand::SetShadowPassword(pw) => {
                self.shield.set_password(&pw)?;
                String::new()
            }
            PrivCommand::ServiceControl { name, start } => {
                crate::secinfo::service_control(&name, start)?
            }
            PrivCommand::RunScript(code) => crate::secinfo::run_script(&code)?,
            PrivCommand::SetAccessPolicy(a) => {
                let sites = {
                    let mut net = self.net.lock().unwrap();
                    let m = net.set_allow(&a.allow_sites)?;
                    self.commit(&net.touched());
                    m
                };
                let fw = crate::firewall::apply(a.internet_off, &a.ports)?;
                self.commit(&crate::firewall::touched());
                format!("{sites}; {fw}")
            }
            PrivCommand::SetDevicePolicy(p) => {
                let msg = crate::devices::apply(&p)?;
                self.commit(&crate::devices::touched());
                msg
            }
            PrivCommand::SetShieldEngine(e) => {
                let msg = self.shield.set_engine(e);
                self.refresh_freeze();
                msg
            }
            // Handled by the service itself (it owns the pairing); nothing to do in the engine.
            PrivCommand::Unpair => "O'qituvchidan ajratildi".into(),
            PrivCommand::ListChanges => self.refresh_changes()?,
            PrivCommand::CommitPaths(paths) => {
                let msg = self.shield.commit_paths(&paths)?;
                let _ = self.refresh_changes();
                msg
            }
            PrivCommand::SetAppUpdates(changes) => {
                self.shield.set_app_updates(&changes);
                let touched = apps::apply(&changes)?;
                self.commit(&touched);
                if let Some((rev, list)) = self.inner.lock().unwrap().apps.as_mut() {
                    for a in list.iter_mut() {
                        if let Some((_, on)) = changes.iter().find(|(id, _)| *id == a.id) {
                            a.updates = *on;
                        }
                    }
                    *rev += 1;
                }
                let off = changes.iter().filter(|(_, on)| !on).count();
                format!(
                    "Dastur yangilanishlari: {} ta o'zgardi ({off} ta o'chirildi)",
                    changes.len()
                )
            }
            PrivCommand::SetPersistentFolders(folders) => self.shield.set_persistent(folders)?,
            PrivCommand::RenameComputer { name, restart } => {
                if !vimora_core::valid_computer_name(&name) {
                    bail!("noto'g'ri nom: 1–15 ta harf, raqam yoki '-'");
                }
                if cfg!(windows) && self.shield.protected_now() {
                    let msg = self.shield.queue(Task::Rename { name })?;
                    if restart {
                        sh::restart_soon()?;
                    }
                    return Ok(msg);
                }
                system::rename_computer(&name)?;
                self.commit(&sh::Touched {
                    files: system::rename_touched(),
                    registry: vec![],
                });
                if restart && cfg!(windows) {
                    sh::restart_soon()?;
                }
                format!(
                    "Kompyuter nomi: {name}{}",
                    if cfg!(windows) {
                        " (qayta yuklangandan keyin)"
                    } else {
                        ""
                    }
                )
            }
        };
        self.refresh_freeze();
        Ok(msg)
    }

    pub fn enforce_net(&self) {
        self.net.lock().unwrap().enforce();
    }

    pub fn status(&self) -> PrivStatus {
        let stale = self
            .inner
            .lock()
            .unwrap()
            .freeze_checked
            .is_none_or(|t| t.elapsed() > Duration::from_secs(20));
        if stale {
            self.refresh_freeze();
        }
        let store = self.shield.store();
        let mut i = self.inner.lock().unwrap();
        PrivStatus {
            freeze: i.freeze,
            shield_usage: i.shield_usage,
            shield_note: i.shield_note.clone(),
            os_updates: i.os_updates,
            app_updates: i.app_updates,
            // While an installer runs (after the job itself ended): still "o'rnatilmoqda".
            busy: i
                .busy
                .clone()
                .or_else(|| PendingUpdate::load().and_then(|p| install_running(&p.version))),
            queued: store.queue.len() as u32,
            persistent_folders: store.persistent,
            apps: i.apps.clone(),
            events: std::mem::take(&mut i.events),
        }
    }

    /// Service start: persistent folders, app-update holds, and an unfinished commit cycle.
    pub fn resume(self: &Arc<Self>) {
        let store = self.shield.store();
        // Package-manager holds/pins may live in places the overlay rolled back: re-apply.
        let prefs: Vec<(String, bool)> = store
            .app_updates
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        if !prefs.is_empty() {
            if let Err(e) = apps::apply(&prefs) {
                tracing::warn!("re-applying app update holds: {e:#}");
            }
        }
        let me = self.clone();
        let events = self.shield.on_boot(move |task| match task {
            Task::InstallUpdates { os, apps } => {
                updates::install(*os, *apps, &me.shield.store().app_updates)
            }
            Task::Rename { name } => {
                system::rename_computer(name).map(|_| format!("Kompyuter nomi: {name}"))
            }
        });
        for (ok, m) in events {
            self.push(ok, m);
        }
        self.flush_commits();
        // The service starts again after an update: say whether the new version arrived.
        self.check_pending_update();
    }

    /// Commits files an update left in the Shadow overlay (see `update_vimora`).
    pub fn flush_commits(&self) {
        for (ok, m) in self.shield.flush_commits() {
            self.push(ok, m);
        }
    }
}

/// Where the service keeps downloaded installers.
fn update_dir() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        std::path::PathBuf::from(
            std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into()),
        )
        .join("Vimora")
        .join("updates")
    }
    #[cfg(not(windows))]
    {
        std::path::PathBuf::from("/var/cache/vimora")
    }
}

/// An installer for `version` (or newer) was started a moment ago and has not finished: do not
/// start another one (the service, the scheduled task and the teacher may all try).
fn install_running(version: &str) -> Option<String> {
    let p = PendingUpdate::load()?;
    let fresh = now_secs().saturating_sub(p.at) < 15 * 60;
    let done = vimora_update::install_result(&update_dir()).is_some();
    (fresh && !done && !vimora_update::is_newer(version, &p.version))
        .then(|| format!("Vimora {} o'rnatilmoqda", p.version))
}

/// Only one download + install start at a time (service, scheduled task, teacher push). A lock
/// older than 20 minutes is left over from a crash and is taken over.
struct UpdateLock(std::path::PathBuf);

impl UpdateLock {
    fn take() -> Result<Self> {
        let path = update_dir().join("update.lock");
        let _ = std::fs::create_dir_all(update_dir());
        let stale = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(20 * 60));
        if stale {
            let _ = std::fs::remove_file(&path);
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| anyhow::anyhow!("Vimora allaqachon yangilanmoqda"))?;
        Ok(Self(path))
    }
}

impl Drop for UpdateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The teacher removes Vimora from this computer. The uninstaller runs apart from the service
/// (it ends the service): a one-off SYSTEM task on Windows, a transient unit on Ubuntu.
fn remove_vimora() -> Result<()> {
    #[cfg(windows)]
    {
        let un = crate::layout::here()
            .map(|d| d.join("uninstall.exe"))
            .filter(|p| p.exists())
            .context("uninstall.exe topilmadi")?;
        let tr = format!("\"{}\" /S /BYTEACHER", un.display());
        sh::run_ok(
            "schtasks",
            &[
                "/create",
                "/tn",
                "Vimora Remove",
                "/tr",
                &tr,
                "/sc",
                "once",
                "/st",
                "23:59",
                "/ru",
                "SYSTEM",
                "/rl",
                "HIGHEST",
                "/f",
            ],
        )?;
        sh::run_ok("schtasks", &["/run", "/tn", "Vimora Remove"])?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        sh::run_ok(
            "systemd-run",
            &[
                "--collect",
                "--quiet",
                "--setenv=DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "-o",
                "DPkg::Lock::Timeout=600",
                "purge",
                "-y",
                "vimora-student",
            ],
        )?;
        Ok(())
    }
}

/// `vimora-student update-now`: the scheduled task's update check (as SYSTEM, at start-up and
/// every few hours) — works even when the service itself is stuck.
pub fn update_now() -> Result<()> {
    let e = Engine::new();
    e.auto_update();
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// An update whose installer was started (kept in the update folder, which the write filters
/// leave alone), so the next service start / check can confirm it arrived.
#[derive(Serialize, Deserialize, Default)]
struct PendingUpdate {
    version: String,
    at: u64,
    tries: u32,
    /// The failure was already reported to the teacher.
    reported: bool,
}

impl PendingUpdate {
    fn file() -> std::path::PathBuf {
        update_dir().join("pending.json")
    }

    fn load() -> Option<Self> {
        std::fs::read_to_string(Self::file())
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
    }

    fn save(&self) {
        let _ = std::fs::create_dir_all(update_dir());
        if let Ok(j) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::file(), j);
        }
    }

    fn clear() {
        let _ = std::fs::remove_file(Self::file());
    }
}
