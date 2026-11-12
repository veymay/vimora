//! Vimora updates over the classroom network. The teacher fetches the newest signed release
//! once and hands it to the student computers that are behind — also those without internet.
//! The computers check the release signature themselves (the key is built into them), so this
//! path can't be used to make them install anything else.
//!
//! Only while automatic Vimora updates are on; only agents that understand the command
//! (0.3.27 and newer); each computer gets one copy per version every few hours at most.

use crate::net::{Level, SharedConfig, SharedHub};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use vimora_core::{Command, TeacherMsg};
use vimora_ui::tf;
use vimora_update::{Package, Release, Target};

/// Agents older than this don't know `Command::InstallUpdate`.
const FIRST_SUPPORTED: &str = "0.3.27";
/// How often the newest release is looked up.
const CHECK_EVERY: Duration = Duration::from_secs(30 * 60);
/// A computer gets the same version again only after this long (it may have failed).
const RESEND_AFTER: Duration = Duration::from_secs(3 * 3600);
/// At most this many computers get a copy per round (one round a minute): the network stays
/// usable during a lesson.
const PER_ROUND: usize = 3;

pub fn start(hub: SharedHub, cfg: SharedConfig) {
    let _ = std::thread::Builder::new()
        .name("vimora-lan-update".into())
        .spawn(move || run(hub, cfg));
}

fn cache_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("vimora")
        .join("lan-update")
}

fn run(hub: SharedHub, cfg: SharedConfig) {
    std::thread::sleep(Duration::from_secs(120));
    let mut last_try: Option<Instant> = None;
    let mut release: Option<(Release, Instant)> = None;
    let mut packages: HashMap<Target, Package> = HashMap::new();
    let mut sent: HashMap<String, (String, Instant)> = HashMap::new();
    loop {
        if cfg.read().unwrap().update_policy.vimora {
            round(&hub, &mut last_try, &mut release, &mut packages, &mut sent);
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn round(
    hub: &SharedHub,
    last_try: &mut Option<Instant>,
    release: &mut Option<(Release, Instant)>,
    packages: &mut HashMap<Target, Package>,
    sent: &mut HashMap<String, (String, Instant)>,
) {
    // Computers that are behind: (id, key, label, version, target).
    let behind = |newest: &str| -> Vec<(u64, String, String, Target)> {
        let h = hub.lock().unwrap();
        h.students
            .iter()
            .filter(|(_, s)| s.status.service && !s.version.is_empty())
            .filter(|(_, s)| !vimora_update::is_newer(FIRST_SUPPORTED, &s.version))
            .filter(|(_, s)| vimora_update::is_newer(newest, &s.version))
            .filter_map(|(id, s)| {
                let t = Target::student_for(&s.info.os)?;
                Some((*id, s.info.key(), s.info.label().to_string(), t))
            })
            .collect()
    };
    // Nothing to do without computers that could take an update.
    if !hub
        .lock()
        .unwrap()
        .students
        .values()
        .any(|s| s.status.service)
    {
        return;
    }
    // Looked up every half hour, also after a failure (no internet: not every minute).
    if last_try.is_none_or(|t| t.elapsed() > CHECK_EVERY) {
        *last_try = Some(Instant::now());
        match vimora_update::latest() {
            Ok(r) => {
                if release
                    .as_ref()
                    .is_none_or(|(old, _)| old.version != r.version)
                {
                    packages.clear();
                }
                *release = Some((r, Instant::now()));
            }
            Err(e) => {
                tracing::debug!("lan update: {e:#}");
                if release.is_none() {
                    return;
                }
            }
        }
    }
    let Some((rel, _)) = release.as_ref() else {
        return;
    };
    let mut todo = behind(&rel.version);
    todo.retain(|(_, key, _, _)| {
        sent.get(key)
            .is_none_or(|(v, at)| *v != rel.version || at.elapsed() > RESEND_AFTER)
    });
    if todo.is_empty() {
        return;
    }
    let mut n = 0;
    for (id, key, label, target) in todo {
        if n >= PER_ROUND {
            break;
        }
        let p = match packages.entry(target) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                match vimora_update::download_target(rel, target, &cache_dir(), &|_, _| {}) {
                    Ok(p) => e.insert(p),
                    Err(err) => {
                        tracing::warn!("lan update: {target:?}: {err:#}");
                        continue;
                    }
                }
            }
        };
        let Ok(data) = std::fs::read(&p.path) else {
            packages.remove(&target);
            continue;
        };
        let p = &packages[&target];
        let msg = TeacherMsg::Command(Command::InstallUpdate {
            name: p.name.clone(),
            data,
            sums: p.sums.clone(),
            sig: p.sig.clone(),
        });
        let mut h = hub.lock().unwrap();
        if h.send(&[id], &msg) > 0 {
            sent.insert(key, (rel.version.clone(), Instant::now()));
            h.log(
                Level::Info,
                tf!(
                    "{name}: Vimora {v} tarmoq orqali yuborildi",
                    name = label,
                    v = p.version.clone()
                ),
            );
            n += 1;
        }
    }
}
