//! Shadow mode: a write filter that throws away every change on restart, so the PC always boots
//! "like new" (the Shadow Defender / Deep Freeze model) — while everything the **teacher**
//! changes is committed and survives.
//!
//! The filtering itself is done by the operating system's write filter:
//! * **Windows** — Unified Write Filter ([`uwf`]): disk-backed overlay, thresholds, exclusions;
//!   single files and registry values can be committed through the filter.
//! * **Ubuntu** — `overlayroot` ([`overlayroot`]): read-only root + RAM overlay; files are
//!   committed by writing them to the real root.
//!
//! On top of it, [`Shield`] keeps a persistent [`Store`] (desired state, persistent folders,
//! per-app update choices, queued teacher tasks) and implements:
//! * **commit** — settings changed by the teacher (policies, hosts, …) are written through;
//! * **commit cycle** — work that can't be committed piecemeal (installing updates, renaming the
//!   computer) is queued; the PC restarts unprotected, runs the queue, re-arms and restarts;
//! * **persistent folders** — chosen folders are never rolled back;
//! * **self-healing** — protection switched off behind the console's back is re-armed;
//! * **capacity watch** — the teacher is warned before the overlay fills up.

#[cfg_attr(windows, allow(dead_code))]
mod overlayroot;
#[cfg_attr(not(windows), allow(dead_code))]
mod uwf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use vimora_core::{FreezeAction, FreezeState, ShieldUsage};

/// What a backend can observe right now.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub state: FreezeState,
    pub usage: Option<ShieldUsage>,
    /// Why it is unsupported, or other detail for the teacher.
    pub note: Option<String>,
}

/// A platform write filter. `enable`/`disable` take effect on the next restart.
pub trait Backend: Send + Sync {
    fn probe(&self) -> Probe;
    /// Arm protection (overlay, exclusions…). Returns a message.
    fn enable(&self, persistent: &[String]) -> Result<String>;
    fn disable(&self) -> Result<String>;
    /// Make the current content of `path` survive restarts (only called while protected).
    fn commit_file(&self, path: &Path) -> Result<()>;
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
}

impl Default for Shield {
    fn default() -> Self {
        Self::new()
    }
}

impl Shield {
    pub fn new() -> Self {
        #[cfg(windows)]
        let backend: Box<dyn Backend> = Box::new(uwf::Uwf);
        #[cfg(not(windows))]
        let backend: Box<dyn Backend> = Box::new(overlayroot::Overlayroot);
        let store = backend
            .read_persistent(STORE)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            backend,
            store: Mutex::new(store),
            last_capacity_alert: Mutex::new(None),
            last_heal: Mutex::new(None),
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

        // 2. Overlay fill level: warn at 75 %, alarm at 90 % (rate-limited).
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

/// Shared helper: a percentage of free space, clamped, in MB.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn overlay_size_mb(free_mb: u64) -> u32 {
    (free_mb * 40 / 100).clamp(2048, 32768) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_size() {
        assert_eq!(overlay_size_mb(1000), 2048);
        assert_eq!(overlay_size_mb(20_000), 8000);
        assert_eq!(overlay_size_mb(1_000_000), 32768);
    }

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
