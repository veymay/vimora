//! Ubuntu backend: `overlayroot`.
//!
//! When protected, `/` is an overlay of the real root (read-only at `/media/root-ro`) and a RAM
//! upper layer (`/media/root-rw`). Commits copy files onto the real root (briefly remounted
//! read-write); persistent folders are bind-mounted from the real root at boot, so writes to
//! them bypass the overlay entirely.

use super::{Backend, Probe};
use crate::sh;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use vimora_core::{ChangeInfo, FreezeState, ShieldUsage};

pub struct Overlayroot;

const CONF: &str = "/etc/overlayroot.conf";
const LOWER: &str = "/media/root-ro";
const UPPER: &str = "/media/root-rw";
const STATE_DIR: &str = "/var/lib/vimora";
const PROTECTED: &str = "overlayroot=\"tmpfs:swap=1,recurse=0\"";
const UNPROTECTED: &str = "overlayroot=\"\"";

/// Is `/` currently an overlay mount?
pub fn root_is_overlay(mounts: &str) -> bool {
    mounts.lines().any(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        f.len() > 2 && f[1] == "/" && f[2] == "overlay"
    })
}

/// Does an overlayroot.conf enable protection?
pub fn conf_enabled(conf: &str) -> bool {
    conf.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .any(|l| {
            l.strip_prefix("overlayroot=").is_some_and(|v| {
                let v = v.trim_matches('"').trim();
                !v.is_empty() && v != "disabled"
            })
        })
}

/// `df -Pm <path>` → (used, size) in MB.
pub fn parse_df(text: &str) -> Option<ShieldUsage> {
    let line = text.lines().nth(1)?;
    let f: Vec<&str> = line.split_whitespace().collect();
    Some(ShieldUsage {
        max_mb: f.get(1)?.parse().ok()?,
        used_mb: f.get(2)?.parse().ok()?,
    })
}

fn frozen_now() -> bool {
    std::fs::read_to_string("/proc/mounts")
        .map(|m| root_is_overlay(&m))
        .unwrap_or(false)
}

/// Path of a file on the *real* root filesystem, as seen from here.
fn real(path: &str) -> PathBuf {
    if frozen_now() {
        Path::new(LOWER).join(path.trim_start_matches('/'))
    } else {
        PathBuf::from(path)
    }
}

/// Runs a shell snippet against the real root (through overlayroot-chroot when protected).
fn on_real_root(script: &str) -> Result<()> {
    if frozen_now() {
        sh::run_ok("overlayroot-chroot", &["sh", "-c", script])?;
    } else {
        sh::run_ok("sh", &["-c", script])?;
    }
    Ok(())
}

fn write_conf(protected: bool) -> Result<()> {
    let line = if protected { PROTECTED } else { UNPROTECTED };
    on_real_root(&format!(
        "touch {CONF}; sed -i '/^overlayroot=/d' {CONF}; echo '{line}' >> {CONF}"
    ))
}

/// Mount options of `path` from /proc/mounts.
pub fn mount_options<'a>(mounts: &'a str, path: &str) -> Option<&'a str> {
    mounts.lines().rev().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        (f.len() > 3 && f[1] == path).then_some(f[3])
    })
}

/// Runs `f` with the real root writable, restoring read-only afterwards (unless it was
/// already writable, e.g. because persistent folders are bind-mounted from it).
fn with_lower_rw<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let was_rw = mount_options(&mounts, LOWER).is_some_and(|o| o.split(',').any(|x| x == "rw"));
    if !was_rw {
        sh::run_ok("mount", &["-o", "remount,rw", LOWER])?;
    }
    let res = f();
    if !was_rw {
        let _ = sh::run("mount", &["-o", "remount,ro", LOWER]);
    }
    res
}

/// Writes `contents` to `path` on the real root (and the live view).
fn write_real(path: &str, contents: &str) -> Result<()> {
    if !frozen_now() {
        if let Some(d) = Path::new(path).parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, contents)?;
        return Ok(());
    }
    let target = real(path);
    with_lower_rw(|| {
        if let Some(d) = target.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&target, contents)?;
        Ok(())
    })?;
    let _ = std::fs::create_dir_all(Path::new(path).parent().unwrap_or(Path::new("/")));
    let _ = std::fs::write(path, contents);
    Ok(())
}

/// Copies a file or a whole folder onto the real root.
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
    } else if meta.is_file() {
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Noise nobody wants to review: logs, caches, runtime state.
pub fn is_noise(path: &str) -> bool {
    [
        "/var/log",
        "/var/cache",
        "/var/lib/vimora",
        "/tmp",
        "/run",
        "/proc",
        "/sys",
        "/dev",
    ]
    .iter()
    .any(|p| path == *p || path.starts_with(&format!("{p}/")))
        || path.contains("/.cache/")
        || path.ends_with("/.cache")
}

/// Walks the overlay's upper layer: files are additions/modifications, 0:0 char devices are
/// deletions. Folders are reported only when they are new and empty.
#[cfg(not(unix))]
fn scan_upper(_upper: &Path) -> Vec<ChangeInfo> {
    vec![]
}

#[cfg(unix)]
fn scan_upper(upper: &Path) -> Vec<ChangeInfo> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use vimora_core::ChangeKind;
    let mut out = vec![];
    let mut stack = vec![upper.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let live = format!("/{}", path.strip_prefix(upper).unwrap_or(&path).display());
            if is_noise(&live) {
                continue;
            }
            if meta.file_type().is_char_device() && meta.rdev() == 0 {
                out.push(ChangeInfo {
                    path: live,
                    kind: ChangeKind::Deleted,
                    size: 0,
                });
            } else if meta.is_dir() {
                stack.push(path);
            } else {
                let existed = Path::new(LOWER).join(live.trim_start_matches('/')).exists();
                out.push(ChangeInfo {
                    path: live,
                    kind: if existed {
                        ChangeKind::Modified
                    } else {
                        ChangeKind::Added
                    },
                    size: meta.len(),
                });
            }
        }
        if out.len() > 3000 {
            break;
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn installed() -> bool {
    real(CONF).exists() || Path::new("/usr/sbin/overlayroot-chroot").exists()
}

impl Backend for Overlayroot {
    fn probe(&self) -> Probe {
        let now = frozen_now();
        let next = std::fs::read_to_string(real(CONF))
            .map(|c| conf_enabled(&c))
            .unwrap_or(false);
        let state = match (now, next) {
            (true, true) => FreezeState::Frozen,
            (true, false) => FreezeState::ThawPending,
            (false, true) => FreezeState::FreezePending,
            (false, false) => FreezeState::Thawed,
        };
        let usage = if now {
            sh::run("df", &["-Pm", UPPER])
                .ok()
                .and_then(|(_, o)| parse_df(&o))
        } else {
            None
        };
        Probe {
            state,
            usage,
            note: (!installed()).then(|| "overlayroot paketi birinchi yoqishda o'rnatiladi".into()),
        }
    }

    fn enable(&self, persistent: &[String]) -> Result<String> {
        if !installed() {
            if frozen_now() {
                bail!("overlayroot o'rnatilmagan");
            }
            sh::run_ok(
                "sh",
                &[
                    "-c",
                    "DEBIAN_FRONTEND=noninteractive apt-get install -y overlayroot",
                ],
            )?;
        }
        for f in persistent {
            let _ = std::fs::create_dir_all(real(f));
        }
        write_conf(true)?;
        Ok("Shadow rejim qayta yuklangandan keyin yoqiladi".into())
    }

    fn disable(&self) -> Result<String> {
        write_conf(false)?;
        Ok("Shadow rejim qayta yuklangandan keyin o'chadi".into())
    }

    fn commit_file(&self, path: &Path) -> Result<()> {
        let src = path.to_string_lossy().into_owned();
        let dst = real(&src);
        with_lower_rw(|| {
            if let Some(d) = dst.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::copy(path, &dst)?;
            Ok(())
        })
    }

    fn commit_path(&self, path: &Path) -> Result<()> {
        let live = path.exists();
        let dst = real(&path.to_string_lossy());
        with_lower_rw(|| {
            if !live {
                // Deleted in the overlay: delete on the real root too.
                if dst.is_dir() {
                    std::fs::remove_dir_all(&dst)?;
                } else if dst.exists() {
                    std::fs::remove_file(&dst)?;
                }
                return Ok(());
            }
            copy_tree(path, &dst)
        })
    }

    fn changes(&self) -> Option<Vec<ChangeInfo>> {
        if !frozen_now() {
            return None;
        }
        Some(scan_upper(&Path::new(UPPER).join("overlay")))
    }

    fn set_persistent(&self, new: &[String], _old: &[String]) -> Result<String> {
        for f in new {
            if !f.starts_with('/') {
                bail!("papka yo'li to'liq bo'lishi kerak: {f}");
            }
        }
        if frozen_now() {
            with_lower_rw(|| {
                for f in new {
                    std::fs::create_dir_all(real(f))?;
                }
                Ok(())
            })?;
        } else {
            for f in new {
                std::fs::create_dir_all(f)?;
            }
        }
        Ok(format!(
            "{} ta papka doim saqlanadi (qayta yuklangandan keyin kuchga kiradi)",
            new.len()
        ))
    }

    fn mount_persistent(&self, folders: &[String]) {
        if !frozen_now() || folders.is_empty() {
            return;
        }
        // The real root must stay writable while folders are bind-mounted from it.
        if let Err(e) = sh::run_ok("mount", &["-o", "remount,rw", LOWER]) {
            tracing::warn!("persistent folders: {e:#}");
            return;
        }
        let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
        for f in folders {
            if mount_options(&mounts, f).is_some() {
                continue; // already bound
            }
            let src = real(f);
            let _ = std::fs::create_dir_all(&src);
            let _ = std::fs::create_dir_all(f);
            if let Err(e) = sh::run_ok("mount", &["--bind", &src.to_string_lossy(), f]) {
                tracing::warn!("bind {f}: {e:#}");
            }
        }
    }

    fn persist(&self, name: &str, contents: &str) -> Result<()> {
        write_real(&format!("{STATE_DIR}/{name}"), contents)
    }

    fn read_persistent(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(real(&format!("{STATE_DIR}/{name}"))).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts() {
        assert!(root_is_overlay(
            "overlayroot / overlay rw,lowerdir=/media/root-ro 0 0\n"
        ));
        assert!(!root_is_overlay("/dev/sda2 / ext4 rw 0 0\n"));
    }

    #[test]
    fn options() {
        let m = "/dev/sda2 /media/root-ro ext4 ro,relatime 0 0\n/dev/sda2 /media/root-ro ext4 rw,relatime 0 0\n";
        assert_eq!(mount_options(m, "/media/root-ro"), Some("rw,relatime"));
        assert_eq!(mount_options(m, "/home"), None);
    }

    #[test]
    fn conf() {
        assert!(conf_enabled("# x\noverlayroot=\"tmpfs:swap=1\"\n"));
        assert!(!conf_enabled("overlayroot=\"\"\n"));
        assert!(!conf_enabled("#overlayroot=\"tmpfs\"\n"));
        assert!(!conf_enabled("overlayroot=disabled\n"));
    }

    #[test]
    fn noise_filter() {
        assert!(is_noise("/var/log/syslog"));
        assert!(is_noise("/home/u/.cache/x"));
        assert!(!is_noise("/home/u/Desktop/a.txt"));
        assert!(!is_noise("/var/logger"));
    }

    #[test]
    fn df() {
        let out = "Filesystem     1048576-blocks  Used Available Capacity Mounted on\ntmpfs-root           3956   412      3544      11% /media/root-rw\n";
        assert_eq!(
            parse_df(out),
            Some(ShieldUsage {
                used_mb: 412,
                max_mb: 3956
            })
        );
    }
}
