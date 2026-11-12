//! Shadow mode: a write filter that throws away every change on restart, so the PC always boots
//! "like new" (the Shadow Defender / Deep Freeze model) — while everything the **teacher**
//! changes is committed and survives.
//!
//! The filtering itself is done by:
//! * **Windows** — Shadow Defender, installed separately and driven through its `CmdTool.exe`
//!   ([`sd`]): enter/exit Shadow Mode, commit files and folders.
//! * **Ubuntu** — `overlayroot` ([`overlayroot`]): read-only root + RAM overlay; files are
//!   committed by writing them to the real root.
//!
//! On top of it, [`Shield`] keeps a persistent [`Store`] (desired state, persistent folders,
//! per-app update choices, queued teacher tasks) and implements:
//! * **commit** — settings changed by the teacher (policies, hosts, …) are written through;
//! * **commit cycle** — work that can't be committed piecemeal (installing updates, renaming the
//!   computer) is queued; the PC restarts unprotected, runs the queue, re-arms and restarts;
//! * **persistent folders** — chosen folders are never rolled back;
//! * **self-healing** — protection switched off behind the teacher's back is re-armed;
//! * **capacity watch** — the teacher is warned before the overlay fills up.

#[cfg_attr(windows, allow(dead_code))]
mod overlayroot;
#[cfg_attr(not(windows), allow(dead_code))]
mod sd;
#[cfg_attr(not(windows), allow(dead_code))]
mod uwf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use vimora_core::{ChangeInfo, FreezeAction, FreezeState, ShieldUsage};

/// What a backend can observe right now.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub state: FreezeState,
    pub usage: Option<ShieldUsage>,
    /// Why it is unsupported, or other detail for the teacher.
    pub note: Option<String>,
}

/// Which Windows write filter the teacher chose (0 = automatic).
static ENGINE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Windows: Shadow Defender or the Unified Write Filter, whichever is chosen (or, on
/// "automatic", Shadow Defender when it is installed and UWF otherwise). Every answer carries
/// a tag — `[SD]` / `[UWF]` — at the start of its note, so the teacher can show which one a
/// computer uses.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Default)]
struct Windows {
    sd: sd::ShadowDefender,
    uwf: uwf::Uwf,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Windows {
    fn uses_uwf(&self) -> bool {
        match ENGINE.load(std::sync::atomic::Ordering::Relaxed) {
            1 => false,
            2 => true,
            _ => !sd::installed(),
        }
    }

    fn pick(&self) -> &dyn Backend {
        if self.uses_uwf() {
            &self.uwf
        } else {
            &self.sd
        }
    }
}

impl Backend for Windows {
    fn probe(&self) -> Probe {
        let mut p = self.pick().probe();
        let tag = if self.uses_uwf() { "[UWF]" } else { "[SD]" };
        p.note = Some(match p.note {
            Some(n) => format!("{tag} {n}"),
            None => tag.to_string(),
        });
        p
    }
    fn enable(&self, persistent: &[String]) -> Result<String> {
        self.pick().enable(persistent)
    }
    fn disable(&self) -> Result<String> {
        self.pick().disable()
    }
    fn commit_file(&self, path: &Path) -> Result<()> {
        self.pick().commit_file(path)
    }
    fn commit_path(&self, path: &Path) -> Result<()> {
        self.pick().commit_path(path)
    }
    fn changes(&self) -> Option<Vec<ChangeInfo>> {
        self.pick().changes()
    }
    fn set_password(&self, pw: &str) -> Result<()> {
        self.sd.set_password(pw)
    }
    fn commit_registry(&self, key: &str, value: &str) -> Result<()> {
        self.pick().commit_registry(key, value)
    }
    fn set_persistent(&self, new: &[String], old: &[String]) -> Result<String> {
        self.pick().set_persistent(new, old)
    }
    fn mount_persistent(&self, folders: &[String]) {
        self.pick().mount_persistent(folders)
    }
    // Vimora's own state lives in %ProgramData%\Vimora whichever filter is used.
    fn persist(&self, name: &str, contents: &str) -> Result<()> {
        self.sd.persist(name, contents)
    }
    fn read_persistent(&self, name: &str) -> Option<String> {
        self.sd.read_persistent(name)
    }
}

/// Shared helper: a percentage of free space, clamped, in MB.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn overlay_size_mb(free_mb: u64) -> u32 {
    (free_mb * 40 / 100).clamp(2048, 32768) as u32
}

/// A platform write filter. `enable`/`disable` take effect on the next restart.
pub trait Backend: Send + Sync {
    fn probe(&self) -> Probe;
    /// Arm protection (overlay, exclusions…). Returns a message.
    fn enable(&self, persistent: &[String]) -> Result<String>;
    fn disable(&self) -> Result<String>;
    /// Make the current content of `path` survive restarts (only called while protected).
    fn commit_file(&self, path: &Path) -> Result<()>;
    /// Make `path` (file, folder or a deletion) survive restarts. Defaults to a file commit.
    fn commit_path(&self, path: &Path) -> Result<()> {
        self.commit_file(path)
    }
    /// What the overlay holds right now. `None` = this backend cannot list it.
    fn changes(&self) -> Option<Vec<ChangeInfo>> {
        None
    }
    /// Store the Shadow Defender password (Windows); other backends ignore it.
    fn set_password(&self, _pw: &str) -> Result<()> {
        Ok(())
    }
    /// Make a registry value survive restarts (Windows only).
    fn commit_registry(&self, _key: &str, _value: &str) -> Result<()> {
        Ok(())
    }
    /// Apply the persistent-folder list (`old` = previous list).
    fn set_persistent(&self, new: &[String], old: &[String]) -> Result<String>;
    /// Called at service start: make persistent folders writable through (Linux bind mounts).
    fn mount_persistent(&self, _folders: &[String]) {}
    /// Store a small file so it survives restarts even while protected.
    fn persist(&self, name: &str, contents: &str) -> Result<()>;
    fn read_persistent(&self, name: &str) -> Option<String>;
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Desired {
    #[default]
    Unset,
    On,
    Off,
}

/// Work done unprotected during a commit cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Task {
    InstallUpdates { os: bool, apps: bool },
    Rename { name: String },
}

/// Everything Shadow mode must remember across restarts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Store {
    pub desired: Desired,
    pub persistent: Vec<String>,
    /// App id → updates allowed.
    pub app_updates: BTreeMap<String, bool>,
    pub queue: Vec<Task>,
    /// A commit cycle was started: the next unprotected boot runs `queue` and re-arms.
    pub cycle: bool,
    /// The teacher turned off Vimora's automatic self-update.
    pub vimora_manual: bool,
    /// Paths to commit as soon as Shadow is active (an update installed under protection).
    pub pending_commit: Vec<String>,
    /// Windows write filter: 0 = automatic, 1 = Shadow Defender, 2 = UWF.
    pub engine: u8,
    /// Boots in a row that found protection wanted but not active (each one arms the filter
    /// and restarts). Capped, so a filter that never comes up can't restart the PC forever.
    pub arm_boots: u8,
}

const STORE: &str = "shield.json";
const WARN_PCT: u32 = 75;
const CRITICAL_PCT: u32 = 90;

pub struct Shield {
    backend: Box<dyn Backend>,
    store: Mutex<Store>,
    last_capacity_alert: Mutex<Option<(u32, Instant)>>,
    /// Last automatic re-arm attempt (retried at most every 10 minutes).
    last_heal: Mutex<Option<Instant>>,
    /// Last commit of the "always kept" paths (every 5 minutes while protected).
    last_keep: Mutex<Option<Instant>>,
}

impl Default for Shield {
    fn default() -> Self {
        Self::new()
    }
}

impl Shield {
    pub fn new() -> Self {
        #[cfg(windows)]
        let backend: Box<dyn Backend> = Box::new(Windows::default());
        #[cfg(not(windows))]
        let backend: Box<dyn Backend> = Box::new(overlayroot::Overlayroot);
        let store = backend
            .read_persistent(STORE)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let store: Store = store;
        ENGINE.store(store.engine, std::sync::atomic::Ordering::Relaxed);
        Self {
            backend,
            store: Mutex::new(store),
            last_capacity_alert: Mutex::new(None),
            last_heal: Mutex::new(None),
            last_keep: Mutex::new(None),
        }
    }

    pub fn store(&self) -> Store {
        self.store.lock().unwrap().clone()
    }

    fn update(&self, f: impl FnOnce(&mut Store)) {
        let snapshot = {
            let mut s = self.store.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        let res = serde_json::to_string(&snapshot)
            .map_err(anyhow::Error::from)
            .and_then(|j| self.backend.persist(STORE, &j));
        if let Err(e) = res {
            tracing::warn!("shield: cannot persist state: {e:#}");
        }
    }

    pub fn probe(&self) -> Probe {
        let mut p = self.backend.probe();
        if self.store.lock().unwrap().cycle && p.state != FreezeState::Unsupported {
            p.state = FreezeState::Maintenance;
        }
        p
    }

    /// The current session runs on the overlay (changes are thrown away on restart).
    pub fn protected_now(&self) -> bool {
        matches!(
            self.backend.probe().state,
            FreezeState::Frozen | FreezeState::ThawPending
        )
    }

    pub fn apply(&self, action: FreezeAction) -> Result<String> {
        match action {
            FreezeAction::Freeze => {
                let msg = self.backend.enable(&self.store().persistent)?;
                self.update(|s| s.desired = Desired::On);
                Ok(msg)
            }
            FreezeAction::Thaw => {
                let msg = self.backend.disable()?;
                self.update(|s| {
                    s.desired = Desired::Off;
                    s.cycle = false;
                    s.queue.clear();
                });
                Ok(msg)
            }
            FreezeAction::Maintenance => self.queue(Task::InstallUpdates {
                os: true,
                apps: true,
            }),
        }
    }

    /// Queues work for a commit cycle (runs unprotected after the next restart).
    pub fn queue(&self, task: Task) -> Result<String> {
        if !self.protected_now() {
            bail!("Shadow rejim yoqilmagan — vazifani to'g'ridan-to'g'ri bajaring");
        }
        self.backend.disable()?;
        self.update(|s| {
            if !s.queue.contains(&task) {
                s.queue.push(task);
            }
            s.cycle = true;
        });
        Ok("Commit sikli: qayta yuklanganda himoyasiz bajariladi va Shadow yana yoqiladi".into())
    }

    /// Writes changed files through the filter (no-op when unprotected).
    pub fn commit_files(&self, paths: &[&Path]) {
        if !self.protected_now() {
            return;
        }
        for p in paths {
            if let Err(e) = self.backend.commit_file(p) {
                tracing::warn!("commit {}: {e:#}", p.display());
            }
        }
    }

    /// Writes changed registry values through the filter (no-op when unprotected).
    pub fn commit_registry(&self, values: &[(String, String)]) {
        if !self.protected_now() {
            return;
        }
        for (k, v) in values {
            if let Err(e) = self.backend.commit_registry(k, v) {
                tracing::warn!("commit {k} {v}: {e:#}");
            }
        }
    }

    /// The overlay's content (Linux). `None` when the platform cannot list it.
    pub fn changes(&self) -> Result<Option<Vec<ChangeInfo>>> {
        if !self.protected_now() {
            bail!("Shadow hozir ishlamayapti — o'zgarishlar allaqachon doim saqlanmoqda");
        }
        Ok(self.backend.changes())
    }

    /// Commits the chosen paths. Returns a summary message.
    pub fn commit_paths(&self, paths: &[String]) -> Result<String> {
        if !self.protected_now() {
            bail!("Shadow hozir ishlamayapti — commit kerak emas, o'zgarishlar saqlanmoqda");
        }
        let (mut ok, mut failed) = (0, vec![]);
        for p in paths {
            match self.backend.commit_path(Path::new(p)) {
                Ok(()) => ok += 1,
                Err(e) => failed.push(format!("{p}: {e:#}")),
            }
        }
        if ok == 0 && !failed.is_empty() {
            bail!("commit bajarilmadi — {}", failed[0]);
        }
        Ok(if failed.is_empty() {
            format!("Commit: {ok} ta o'zgarish Shadow ichiga qo'shildi")
        } else {
            format!(
                "Commit: {ok} ta saqlandi, {} ta xato — {}",
                failed.len(),
                failed[0]
            )
        })
    }

    pub fn set_persistent(&self, folders: Vec<String>) -> Result<String> {
        let clean: Vec<String> = folders
            .into_iter()
            .map(|f| f.trim().trim_end_matches(['\\', '/']).to_string())
            .filter(|f| !f.is_empty())
            .collect();
        let old = self.store().persistent;
        let msg = self.backend.set_persistent(&clean, &old)?;
        self.update(|s| s.persistent = clean);
        Ok(msg)
    }

    /// Chooses the Windows write filter (see [`Store::engine`]).
    pub fn set_engine(&self, engine: u8) -> String {
        let engine = engine.min(2);
        ENGINE.store(engine, std::sync::atomic::Ordering::Relaxed);
        self.update(|s| s.engine = engine);
        match engine {
            1 => "Muzlatish usuli: Shadow Defender".into(),
            2 => "Muzlatish usuli: UWF (Windows)".into(),
            _ => "Muzlatish usuli: avtomatik".into(),
        }
    }

    pub fn set_password(&self, pw: &str) -> Result<()> {
        self.backend.set_password(pw)
    }

    /// Remembers paths to commit once Shadow protects the PC (e.g. the new Vimora files after a
    /// self-update installed while protected).
    pub fn queue_commit(&self, paths: Vec<String>) {
        self.update(|s| {
            for p in paths {
                if !s.pending_commit.contains(&p) {
                    s.pending_commit.push(p);
                }
            }
        });
    }

    /// Commits the queued paths if Shadow is active now. Returns events for the teacher.
    pub fn flush_commits(&self) -> Vec<(bool, String)> {
        let pending = self.store().pending_commit;
        if pending.is_empty() || !self.protected_now() {
            return vec![];
        }
        let res = self.commit_paths(&pending);
        self.update(|s| s.pending_commit.clear());
        match res {
            Ok(m) => vec![(true, m)],
            Err(e) => vec![(false, format!("{e:#}"))],
        }
    }

    pub fn set_vimora_auto(&self, on: bool) {
        self.update(|s| s.vimora_manual = !on);
    }

    pub fn set_app_updates(&self, changes: &[(String, bool)]) {
        self.update(|s| {
            for (id, on) in changes {
                s.app_updates.insert(id.clone(), *on);
            }
        });
    }

    /// Service start: bind persistent folders and finish a commit cycle if one is due.
    /// `run` executes one queued task.
    pub fn on_boot(&self, run: impl Fn(&Task) -> Result<String>) -> Vec<(bool, String)> {
        let store = self.store();
        self.backend.mount_persistent(&store.persistent);
        if !store.cycle && store.desired == Desired::On {
            return self.finish_arming(store.arm_boots);
        }
        if !store.cycle || self.protected_now() {
            return vec![];
        }
        // Unprotected boot of a commit cycle: run the queue, re-arm, restart.
        let mut events = vec![];
        for t in &store.queue {
            match run(t) {
                Ok(m) => events.push((true, m)),
                Err(e) => events.push((false, format!("{e:#}"))),
            }
        }
        match self.backend.enable(&store.persistent) {
            Ok(_) => {
                self.update(|s| {
                    s.queue.clear();
                    s.cycle = false;
                    s.desired = Desired::On;
                });
                events.push((true, "Commit sikli tugadi — Shadow qayta yoqilmoqda".into()));
                if let Err(e) = crate::sh::restart_soon() {
                    events.push((false, format!("qayta yuklab bo'lmadi: {e:#}")));
                }
            }
            Err(e) => events.push((false, format!("Shadow ni qayta yoqib bo'lmadi: {e:#}"))),
        }
        events
    }

    /// Boot with protection wanted: when it is not active yet (UWF just installed, or armed
    /// without the restart), arm it and restart — at most 3 boots in a row.
    fn finish_arming(&self, tries: u8) -> Vec<(bool, String)> {
        let state = self.backend.probe().state;
        if matches!(state, FreezeState::Frozen | FreezeState::ThawPending) {
            if tries > 0 {
                self.update(|s| s.arm_boots = 0);
                return vec![(
                    true,
                    "Muzlatish yoqildi: endi qayta yuklanganda barcha o'zgarishlar o'chadi".into(),
                )];
            }
            return vec![];
        }
        if !matches!(state, FreezeState::Thawed | FreezeState::FreezePending) {
            return vec![];
        }
        if tries >= 3 {
            return vec![(
                false,
                "Muzlatishni yoqib bo'lmadi: 3 marta qayta yuklandi, filtr ishga tushmadi".into(),
            )];
        }
        self.update(|s| s.arm_boots = tries + 1);
        let mut events = vec![];
        if state == FreezeState::Thawed {
            if let Err(e) = self.backend.enable(&self.store().persistent) {
                return vec![(false, format!("Muzlatishni sozlab bo'lmadi: {e:#}"))];
            }
        }
        events.push((
            true,
            "Muzlatish sozlandi — kompyuter qayta yuklanmoqda".into(),
        ));
        if let Err(e) = crate::sh::restart_soon() {
            events.push((false, format!("qayta yuklab bo'lmadi: {e:#}")));
        }
        events
    }

    /// Self-healing + capacity watch. Returns events for the teacher: (ok, message).
    pub fn reconcile(&self) -> Vec<(bool, String)> {
        let mut events = vec![];
        let p = self.backend.probe();
        let store = self.store();

        // 1. Desired "on" but the next boot would be unprotected (and no cycle planned) → re-arm.
        let heal_due = self
            .last_heal
            .lock()
            .unwrap()
            .is_none_or(|t| t.elapsed() > Duration::from_secs(600));
        if heal_due
            && store.desired == Desired::On
            && !store.cycle
            && matches!(p.state, FreezeState::Thawed | FreezeState::ThawPending)
        {
            *self.last_heal.lock().unwrap() = Some(Instant::now());
            match self.backend.enable(&store.persistent) {
                Ok(_) => events.push((
                    false,
                    "Shadow himoyasi o'chirilgan edi — avtomatik qayta yoqildi".to_string(),
                )),
                Err(e) => events.push((false, format!("Shadow himoyasini tiklab bo'lmadi: {e:#}"))),
            }
        }

        // 2. "Doim saqlanadigan" files and folders: committed every 5 minutes while protected
        //    (Shadow Defender has no exclusion list we can set; on UWF they are excluded and
        //    the commit is a harmless no-op).
        let keep_due = self
            .last_keep
            .lock()
            .unwrap()
            .is_none_or(|t| t.elapsed() > Duration::from_secs(300));
        if keep_due
            && !store.persistent.is_empty()
            && matches!(p.state, FreezeState::Frozen | FreezeState::ThawPending)
        {
            *self.last_keep.lock().unwrap() = Some(Instant::now());
            for path in &store.persistent {
                let path = Path::new(path);
                if path.exists() {
                    if let Err(e) = self.backend.commit_path(path) {
                        tracing::debug!("keep {}: {e:#}", path.display());
                    }
                }
            }
        }

        // 3. Overlay fill level: warn at 75 %, alarm at 90 % (rate-limited).
        if let Some(u) = p.usage.filter(|u| u.max_mb > 0) {
            let pct = u.percent();
            let level = if pct >= CRITICAL_PCT {
                2
            } else if pct >= WARN_PCT {
                1
            } else {
                0
            };
            let mut last = self.last_capacity_alert.lock().unwrap();
            let due = match *last {
                Some((l, t)) => {
                    level > l
                        || t.elapsed() > Duration::from_secs(if level == 2 { 300 } else { 1800 })
                }
                None => true,
            };
            if level > 0 && due {
                *last = Some((level, Instant::now()));
                let what = if level == 2 {
                    "deyarli to'ldi — kompyuterni qayta yuklang"
                } else {
                    "to'lib bormoqda"
                };
                events.push((
                    false,
                    format!(
                        "Shadow xotirasi {pct}% ({} / {} MB) {what}",
                        u.used_mb, u.max_mb
                    ),
                ));
            } else if level == 0 {
                *last = None;
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip_and_defaults() {
        let s: Store = serde_json::from_str(r#"{"desired":"On"}"#).unwrap();
        assert_eq!(s.desired, Desired::On);
        assert!(s.queue.is_empty() && !s.cycle);
        let t = Store {
            queue: vec![Task::Rename {
                name: "PC-1".into(),
            }],
            cycle: true,
            ..Default::default()
        };
        let back: Store = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back, t);
    }
}
