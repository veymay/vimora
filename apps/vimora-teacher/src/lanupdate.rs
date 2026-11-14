//! Vimora updates over the classroom network. The teacher fetches the newest signed release
//! once and hands it to the student computers that are behind — also those without internet.
//! The computers check the release signature themselves (the key is built into them), so this
//! path can't be used to make them install anything else.
//!
//! Only to computers whose automatic Vimora updates are on; only agents that understand the command
//! (0.3.27 and newer); each computer gets one copy per version every few hours at most.

use crate::net::{Level, SharedConfig, SharedHub};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use vimora_core::{Command, TeacherMsg};
use vimora_ui::tf;
use vimora_update::{Package, Release, Target};

/// Agents older than this don't know `Command::InstallUpdate`.
const FIRST_SUPPORTED: &str = "0.3.27";
/// Agents from this version can take the bare program file (the second way to update).
const SECOND_WAY_FROM: &str = "0.3.41";
/// If the installer did not do it in this time, the next copy is the program file itself.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often the newest release is looked up.
const CHECK_EVERY: Duration = Duration::from_secs(30 * 60);
/// A computer gets the same version again only after this long (it may have failed).
const RESEND_AFTER: Duration = Duration::from_secs(3 * 3600);
/// At most this many computers get a copy per round (one round a minute): the network stays
/// usable during a lesson.
const PER_ROUND: usize = 3;

/// What to send to a computer: the installer, or (a Windows computer that can take it, and
/// the installer was sent before) the program file itself.
fn target_for(os: &str, version: &str, again: bool) -> Option<Target> {
    let t = Target::student_for(os)?;
    if t == Target::Windows && again && !vimora_update::is_newer(SECOND_WAY_FROM, version) {
        Some(Target::StudentBinary)
    } else {
        Some(t)
    }
}

/// "Yangilash" in the teacher: the computer tries by itself first (it downloads); after
/// `delay` the ones still behind get the file over the network (the program file when they can
/// take it: it does not depend on an installer).
pub fn push_after(hub: SharedHub, ids: Vec<u64>, delay: Duration) {
    let _ = std::thread::Builder::new()
        .name("vimora-push-update".into())
        .spawn(move || {
            std::thread::sleep(delay);
            let Ok(rel) = vimora_update::latest() else {
                return;
            };
            let mut packages: HashMap<Target, Package> = HashMap::new();
            for id in ids {
                let Some((os, version)) = hub
                    .lock()
                    .unwrap()
                    .students
                    .get(&id)
                    .filter(|s| s.status.service && !s.version.is_empty())
                    .map(|s| (s.info.os.clone(), s.version.clone()))
                else {
                    continue;
                };
                if !vimora_update::is_newer(&rel.version, &version)
                    || vimora_update::is_newer(FIRST_SUPPORTED, &version)
                {
                    continue;
                }
                let Some(target) = target_for(&os, &version, true) else {
                    continue;
                };
                if !packages.contains_key(&target) {
                    match vimora_update::download_target(&rel, target, &cache_dir(), &|_, _| {}) {
                        Ok(p) => {
                            packages.insert(target, p);
                        }
                        Err(e) => {
                            tracing::warn!("push update: {target:?}: {e:#}");
                            continue;
                        }
                    }
                }
                let p = &packages[&target];
                let Ok(data) = std::fs::read(&p.path) else {
                    continue;
                };
                let msg = TeacherMsg::Command(Command::InstallUpdate {
                    name: p.name.clone(),
                    data,
                    sums: p.sums.clone(),
                    sig: p.sig.clone(),
                });
                hub.lock().unwrap().send(&[id], &msg);
            }
        });
}

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
    let mut sent: HashMap<String, (String, Instant, u32)> = HashMap::new();
    loop {
        round(
            &hub,
            &cfg,
            &mut last_try,
            &mut release,
            &mut packages,
            &mut sent,
        );
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn round(
    hub: &SharedHub,
    cfg: &SharedConfig,
    last_try: &mut Option<Instant>,
    release: &mut Option<(Release, Instant)>,
    packages: &mut HashMap<Target, Package>,
    sent: &mut HashMap<String, (String, Instant, u32)>,
) {
    // Computers that are behind: (id, key, label, version, target).
    let behind = |newest: &str| -> Vec<(u64, String, String, String, String)> {
        let c = cfg.read().unwrap();
        let h = hub.lock().unwrap();
        h.students
            .iter()
            .filter(|(_, s)| s.status.service && !s.version.is_empty())
            .filter(|(_, s)| !vimora_update::is_newer(FIRST_SUPPORTED, &s.version))
            .filter(|(_, s)| vimora_update::is_newer(newest, &s.version))
            // Only where automatic Vimora updates are on (per computer).
            .filter(|(_, s)| c.policy_of(&s.info.key()).vimora)
            .filter_map(|(id, s)| {
                Target::student_for(&s.info.os)?;
                Some((
                    *id,
                    s.info.key(),
                    s.info.label().to_string(),
                    s.info.os.clone(),
                    s.version.clone(),
                ))
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
    todo.retain(|(_, key, _, _, _)| {
        sent.get(key).is_none_or(|(v, at, tries)| {
            *v != rel.version
                || at.elapsed() > RESEND_AFTER
                // The first copy (the installer) did not do it: the next one comes sooner.
                || (*tries == 1 && at.elapsed() > RETRY_AFTER)
        })
    });
    if todo.is_empty() {
        return;
    }
    let mut n = 0;
    for (id, key, label, os, version) in todo {
        if n >= PER_ROUND {
            break;
        }
        let again = sent
            .get(&key)
            .is_some_and(|(v, _, tries)| *v == rel.version && *tries >= 1);
        let Some(target) = target_for(&os, &version, again) else {
            continue;
        };
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
            let tries = sent
                .get(&key)
                .filter(|(v, _, _)| *v == rel.version)
                .map_or(0, |(_, _, t)| *t)
                + 1;
            sent.insert(key, (rel.version.clone(), Instant::now(), tries));
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
