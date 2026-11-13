//! Installed applications and per-application update control.
//!
//! * Windows: `winget list` (ids, versions, available upgrades); updates are blocked with
//!   `winget pin` and, for browsers that update themselves, their own update policies.
//! * Ubuntu: manually installed apt packages + snaps; blocked with `apt-mark hold` /
//!   `snap refresh --hold`.
//!
//! The teacher's choices are stored by the Shadow store and re-applied at every boot, so they
//! hold even when a protected PC throws away the package manager's own state.

use crate::sh::{self, Touched};
use anyhow::Result;
use std::collections::BTreeMap;
use vimora_core::AppInfo;

pub fn list(prefs: &BTreeMap<String, bool>) -> Result<Vec<AppInfo>> {
    let mut apps = imp::list()?;
    for a in &mut apps {
        a.updates = prefs.get(&a.id).copied().unwrap_or(true);
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok(apps)
}

/// Applies update permissions. Returns what was touched (for Shadow-mode commits).
pub fn apply(changes: &[(String, bool)]) -> Result<Touched> {
    let mut touched = Touched::default();
    for (id, on) in changes {
        imp::set(id, *on, &mut touched)?;
    }
    Ok(touched)
}

/// Ids whose updates the teacher switched off.
pub fn blocked(prefs: &BTreeMap<String, bool>) -> Vec<String> {
    prefs
        .iter()
        .filter(|(_, on)| !**on)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Uninstalls an application silently (winget on Windows; snap or apt on Ubuntu).
pub fn uninstall(id: &str) -> Result<String> {
    if id.trim().is_empty() || id.starts_with('-') {
        anyhow::bail!("dastur noto'g'ri");
    }
    imp::uninstall(id)
}

/// Updates these applications now (winget on Windows; snap or apt on Ubuntu).
pub fn upgrade(ids: &[String]) -> Result<String> {
    let mut done = 0;
    let mut failed = vec![];
    for id in ids {
        if id.trim().is_empty() || id.starts_with('-') {
            continue;
        }
        match imp::upgrade_one(id) {
            Ok(()) => done += 1,
            Err(e) => failed.push(format!("{id}: {}", short_error(&format!("{e:#}")))),
        }
    }
    if done == 0 && !failed.is_empty() {
        anyhow::bail!("{}", failed.join("; "));
    }
    if failed.is_empty() {
        Ok(format!("{done} ta dastur yangilandi"))
    } else {
        Ok(format!(
            "{done} ta dastur yangilandi, {} tasi o'rnatilmadi ({})",
            failed.len(),
            failed.join("; ")
        ))
    }
}

/// An installer's long complaint in a few words: its exit code when it has one, else the first
/// part of the text without file paths ("… log is available at C:\…").
fn short_error(text: &str) -> String {
    let lower = text.to_lowercase();
    if let Some(i) = lower.find("exit code:") {
        let code: String = text[i + 10..]
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        if !code.is_empty() {
            return format!("o'rnatuvchi xatosi ({code})");
        }
    }
    let cut = lower.find(" · installer log").unwrap_or(text.len());
    text[..cut].chars().take(100).collect()
}

/// Upgrades every application with an available update except the blocked ones.
pub fn upgrade_allowed(prefs: &BTreeMap<String, bool>) -> Result<String> {
    imp::upgrade_allowed(&blocked(prefs))
}

/// Parses winget's fixed-width table using the header's column offsets.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_winget_table(text: &str) -> Vec<AppInfo> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(h) = lines
        .iter()
        .position(|l| l.contains("Id") && l.contains("Version") && l.contains("Name"))
    else {
        return vec![];
    };
    let header = lines[h].trim_start_matches(|c: char| !c.is_alphanumeric());
    let offset = lines[h].len() - header.len();
    let col = |name: &str| header.find(name).map(|i| i + offset);
    let (Some(id), Some(ver)) = (col("Id"), col("Version")) else {
        return vec![];
    };
    let avail = col("Available");
    let src = col("Source");
    let slice = |l: &str, a: usize, b: Option<usize>| -> String {
        let chars: Vec<char> = l.chars().collect();
        let end = b.unwrap_or(chars.len()).min(chars.len());
        if a >= end {
            return String::new();
        }
        chars[a..end].iter().collect::<String>().trim().to_string()
    };
    lines[h + 1..]
        .iter()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('-'))
        .filter(|l| l.chars().count() > ver)
        .filter_map(|l| {
            let name = slice(l, 0, Some(id));
            let id_s = slice(l, id, Some(ver));
            let version = slice(l, ver, avail.or(src));
            let available = avail.map(|a| slice(l, a, src)).filter(|s| !s.is_empty());
            let source = src.map(|s| slice(l, s, None)).unwrap_or_default();
            (!id_s.is_empty() && !name.is_empty()).then_some(AppInfo {
                id: id_s,
                name,
                version,
                available,
                source: if source.is_empty() {
                    "winget".into()
                } else {
                    source
                },
                updates: true,
            })
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use super::*;

    /// Browsers that update themselves: (winget id, policy key, value, "on" value, "off" value).
    const SELF_UPDATERS: &[(&str, &str, &str, u32, u32)] = &[
        (
            "Google.Chrome",
            r"HKLM\SOFTWARE\Policies\Google\Update",
            "Update{8A69D345-D564-463C-AFF1-A69D9E530F96}",
            1,
            0,
        ),
        (
            "Microsoft.Edge",
            r"HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate",
            "Update{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}",
            1,
            0,
        ),
        (
            "Mozilla.Firefox",
            r"HKLM\SOFTWARE\Policies\Mozilla\Firefox",
            "DisableAppUpdate",
            0,
            1,
        ),
    ];

    fn winget(args: &[&str]) -> Result<(bool, String)> {
        sh::run(&crate::updates::winget_path(), args)
    }

    pub fn upgrade_one(id: &str) -> Result<()> {
        let (ok, out) = winget(&[
            "upgrade",
            "--id",
            id,
            "--exact",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])?;
        if !ok {
            anyhow::bail!("{}", sh::tail(&out, 2));
        }
        Ok(())
    }

    pub fn uninstall(id: &str) -> Result<String> {
        let (ok, out) = winget(&[
            "uninstall",
            "--id",
            id,
            "--exact",
            "--silent",
            "--force",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])?;
        if !ok {
            anyhow::bail!("o'chirilmadi: {}", sh::tail(&out, 2));
        }
        Ok(format!("{id} o'chirildi"))
    }

    pub fn list() -> Result<Vec<AppInfo>> {
        let (_, out) = winget(&[
            "list",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])?;
        let apps = parse_winget_table(&out);
        if apps.is_empty() {
            anyhow::bail!("winget ro'yxat bermadi: {}", sh::tail(&out, 2));
        }
        Ok(apps)
    }

    pub fn set(id: &str, on: bool, touched: &mut Touched) -> Result<()> {
        if on {
            let _ = winget(&[
                "pin",
                "remove",
                "--id",
                id,
                "--exact",
                "--disable-interactivity",
            ]);
        } else {
            let _ = winget(&[
                "pin",
                "add",
                "--id",
                id,
                "--exact",
                "--blocking",
                "--force",
                "--disable-interactivity",
            ]);
        }
        if let Some((_, key, value, yes, no)) =
            SELF_UPDATERS.iter().find(|s| s.0.eq_ignore_ascii_case(id))
        {
            let v = if on { *yes } else { *no };
            sh::run_ok(
                "reg",
                &[
                    "add",
                    key,
                    "/v",
                    value,
                    "/t",
                    "REG_DWORD",
                    "/d",
                    &v.to_string(),
                    "/f",
                ],
            )?;
            touched.registry.push((key.to_string(), value.to_string()));
        }
        Ok(())
    }

    pub fn upgrade_allowed(blocked: &[String]) -> Result<String> {
        let (_, out) = winget(&[
            "upgrade",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])?;
        let pending: Vec<AppInfo> = parse_winget_table(&out)
            .into_iter()
            .filter(|a| !blocked.iter().any(|b| b.eq_ignore_ascii_case(&a.id)))
            .collect();
        let mut done = 0;
        for a in &pending {
            let (ok, _) = winget(&[
                "upgrade",
                "--id",
                &a.id,
                "--exact",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity",
            ])?;
            done += ok as usize;
        }
        Ok(format!("dasturlar: {done}/{} yangilandi", pending.len()))
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::collections::{HashMap, HashSet};

    pub fn upgrade_one(id: &str) -> Result<()> {
        let snap = sh::run("snap", &["list", id]).is_ok_and(|(ok, _)| ok);
        let (ok, out) = if snap {
            sh::run("snap", &["refresh", id])?
        } else {
            sh::run(
                "apt-get",
                &[
                    "install",
                    "--only-upgrade",
                    "-y",
                    "-o",
                    "DPkg::Lock::Timeout=300",
                    id,
                ],
            )?
        };
        if !ok {
            anyhow::bail!("{}", sh::tail(&out, 2));
        }
        Ok(())
    }

    pub fn uninstall(id: &str) -> Result<String> {
        let snap = sh::run("snap", &["list", id]).is_ok_and(|(ok, _)| ok);
        let (ok, out) = if snap {
            sh::run("snap", &["remove", id])?
        } else {
            sh::run(
                "apt-get",
                &["remove", "-y", "-o", "DPkg::Lock::Timeout=300", id],
            )?
        };
        if !ok {
            anyhow::bail!("o'chirilmadi: {}", sh::tail(&out, 2));
        }
        Ok(format!("{id} o'chirildi"))
    }

    pub fn list() -> Result<Vec<AppInfo>> {
        let (_, manual) = sh::run("apt-mark", &["showmanual"])?;
        let manual: HashSet<&str> = manual.lines().map(str::trim).collect();
        let (_, pkgs) = sh::run("dpkg-query", &["-W", "-f", "${Package}\t${Version}\n"])?;
        let (_, upg) = sh::run("apt", &["list", "--upgradable"]).unwrap_or((false, String::new()));
        let available: HashMap<String, String> = upg
            .lines()
            .filter_map(|l| {
                let (name, rest) = l.split_once('/')?;
                Some((
                    name.to_string(),
                    rest.split_whitespace().nth(1)?.to_string(),
                ))
            })
            .collect();
        let mut apps: Vec<AppInfo> = pkgs
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter(|(p, _)| manual.contains(p))
            .map(|(p, v)| AppInfo {
                id: p.to_string(),
                name: p.to_string(),
                version: v.to_string(),
                available: available.get(p).cloned(),
                source: "apt".into(),
                updates: true,
            })
            .collect();
        if let Ok((_, snaps)) = sh::run("snap", &["list"]) {
            let (_, refresh) =
                sh::run("snap", &["refresh", "--list"]).unwrap_or((false, String::new()));
            let snap_avail: HashMap<&str, &str> = refresh
                .lines()
                .skip(1)
                .filter_map(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    Some((*f.first()?, *f.get(1)?))
                })
                .collect();
            apps.extend(snaps.lines().skip(1).filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                let (name, version) = (*f.first()?, *f.get(1)?);
                Some(AppInfo {
                    id: format!("snap:{name}"),
                    name: name.to_string(),
                    version: version.to_string(),
                    available: snap_avail.get(name).map(|s| s.to_string()),
                    source: "snap".into(),
                    updates: true,
                })
            }));
        }
        Ok(apps)
    }

    pub fn set(id: &str, on: bool, touched: &mut Touched) -> Result<()> {
        if let Some(snap) = id.strip_prefix("snap:") {
            let arg = if on { "--unhold" } else { "--hold" };
            sh::run_ok("snap", &["refresh", arg, snap])?;
        } else {
            sh::run_ok("apt-mark", &[if on { "unhold" } else { "hold" }, id])?;
            touched.files.push("/var/lib/dpkg/status".into());
        }
        Ok(())
    }

    pub fn upgrade_allowed(_blocked: &[String]) -> Result<String> {
        // apt and snap honour the holds set by `set`.
        let (_, out) = sh::run("snap", &["refresh"]).unwrap_or((true, String::new()));
        Ok(format!("snap: {}", sh::tail(&out, 1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winget_table() {
        let out = "   - \\ \n\
Name                     Id                      Version       Available     Source\n\
-------------------------------------------------------------------------------------\n\
Google Chrome            Google.Chrome           119.0.6045    120.0.6099    winget\n\
7-Zip 23.01 (x64)        7zip.7zip               23.01                       winget\n\
Some Local App           ARP\\Machine\\X64\\Foo      1.0\n";
        let apps = parse_winget_table(out);
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].id, "Google.Chrome");
        assert_eq!(apps[0].available.as_deref(), Some("120.0.6099"));
        assert_eq!(apps[1].name, "7-Zip 23.01 (x64)");
        assert_eq!(apps[1].available, None);
        assert_eq!(apps[2].version, "1.0");
    }

    #[test]
    fn blocked_ids() {
        let prefs: BTreeMap<String, bool> =
            [("a".to_string(), true), ("b".to_string(), false)].into();
        assert_eq!(blocked(&prefs), vec!["b".to_string()]);
    }
}
