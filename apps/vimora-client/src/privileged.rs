//! Executes [`PrivCommand`]s: Shadow mode, updates, applications, renaming, power, NetControl.
//!
//! Lives inside the Vimora service (SYSTEM / root). The user-session agent only forwards the
//! console's signed commands to it, so students can't trigger these by themselves.
//!
//! **Teacher changes always persist.** While Shadow mode protects the PC, every setting this
//! engine changes is committed through the write filter, and work that can't be committed
//! piecemeal (installing updates, renaming) is queued for a commit cycle.

use crate::shield::{Shield, Task};
use crate::{apps, netcontrol::NetControl, sh, system, updates};
use anyhow::{bail, Result};
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
    /// Shadow overlay content and its revision.
    pub changes: Option<(u64, Vec<ChangeInfo>)>,
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
    changes: Option<(u64, Vec<ChangeInfo>)>,
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

    /// Re-reads the overlay content into the status.
    fn refresh_changes(&self) -> Result<String> {
        let list = self.shield.changes()?;
        let Some(list) = list else {
            bail!("bu tizimda overlay tarkibini ko'rib bo'lmaydi (Windows UWF) — yo'lni qo'lda kiriting");
        };
        let n = list.len();
        let mut i = self.inner.lock().unwrap();
        let rev = i.changes.as_ref().map_or(1, |(r, _)| r + 1);
        i.changes = Some((rev, list));
        Ok(format!("{n} ta o'zgarish topildi"))
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
        self.refresh_freeze();
    }

    /// Background self-update (service, every few hours) unless the teacher turned it off.
    pub fn auto_update(self: &Arc<Self>) {
        if self.shield.store().vimora_manual || self.inner.lock().unwrap().busy.is_some() {
            return;
        }
        match self.update_vimora() {
            Ok(m) if m.contains("o'rnatilmoqda") => self.push(true, m),
            Ok(_) => {}
            Err(e) => tracing::info!("self-update skipped: {e:#}"),
        }
    }

    /// Installs the newest GitHub release if it is newer than this build.
    fn update_vimora(&self) -> Result<String> {
        use vimora_update::Role;
        let rel = vimora_update::latest()?;
        if !rel.is_newer() {
            return Ok(format!(
                "Vimora {} — eng so'nggi versiya",
                vimora_update::current()
            ));
        }
        if self.shield.protected_now() {
            bail!(
                "Shadow yoqilgan: Vimora {} ni o'rnatish uchun avval Shadow'ni o'chiring",
                rel.version
            );
        }
        let path = vimora_update::download(&rel, Role::Student, &update_dir(), &|_, _| {})?;
        vimora_update::install_student(&path)?;
        Ok(format!("Vimora {} o'rnatilmoqda", rel.version))
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
            PrivCommand::UpdateVimora => self.job("Vimora yangilanmoqda", |e| e.update_vimora())?,
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
            PrivCommand::SetShadowPassword(pw) => {
                self.shield.set_password(&pw)?;
                String::new()
            }
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
            busy: i.busy.clone(),
            queued: store.queue.len() as u32,
            persistent_folders: store.persistent,
            apps: i.apps.clone(),
            changes: i.changes.clone(),
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
