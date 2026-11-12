//! OS and application updates, controlled from the teacher.
//!
//! * Windows: Windows Update through its COM API (PowerShell), policies in
//!   `HKLM\SOFTWARE\Policies`, apps through `winget` plus Store/Chrome/Edge update policies.
//! * Ubuntu: apt (`unattended-upgrades` policy) and snap.

use crate::sh;
use anyhow::Result;
use vimora_core::{OsUpdateMode, UpdatePolicy};

pub struct Pending {
    pub os: u32,
    pub apps: u32,
}

pub fn set_policy(p: UpdatePolicy) -> Result<String> {
    imp::set_policy(p)?;
    let os = match p.os {
        OsUpdateMode::Auto => "avtomatik",
        OsUpdateMode::Manual => "faqat o'qituvchidan",
        OsUpdateMode::Disabled => "o'chirilgan",
    };
    Ok(format!(
        "Yangilanish siyosati: tizim — {os}, dasturlar — {}",
        if p.apps_auto {
            "avtomatik"
        } else {
            "o'qituvchidan"
        }
    ))
}

pub fn check() -> Result<Pending> {
    imp::check()
}

/// Installs pending updates (apps: only those the teacher didn't block). Returns a summary.
pub fn install(
    os: bool,
    apps: bool,
    prefs: &std::collections::BTreeMap<String, bool>,
) -> Result<String> {
    let mut parts = vec![];
    if os {
        parts.push(imp::install_os()?);
    }
    if apps {
        parts.push(crate::apps::upgrade_allowed(prefs)?);
    }
    Ok(parts.join("; "))
}

/// Settings `set_policy` writes (committed while Shadow mode is on).
pub fn policy_touched() -> crate::sh::Touched {
    imp::policy_touched()
}

#[cfg(windows)]
pub use imp::winget_path;

pub fn reboot_required() -> bool {
    imp::reboot_required()
}

#[cfg(windows)]
mod imp {
    use super::*;

    const WU: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";

    fn reg_dword(key: &str, name: &str, v: u32) -> Result<()> {
        sh::run_ok(
            "reg",
            &[
                "add",
                key,
                "/v",
                name,
                "/t",
                "REG_DWORD",
                "/d",
                &v.to_string(),
                "/f",
            ],
        )
        .map(|_| ())
    }

    pub fn set_policy(p: UpdatePolicy) -> Result<()> {
        match p.os {
            OsUpdateMode::Auto => {
                reg_dword(WU, "NoAutoUpdate", 0)?;
                reg_dword(WU, "AUOptions", 4)?;
            }
            OsUpdateMode::Manual => {
                reg_dword(WU, "NoAutoUpdate", 0)?;
                reg_dword(WU, "AUOptions", 2)?; // notify only; installs come from the teacher
            }
            OsUpdateMode::Disabled => reg_dword(WU, "NoAutoUpdate", 1)?,
        }
        let on = p.apps_auto;
        let _ = reg_dword(
            r"HKLM\SOFTWARE\Policies\Microsoft\WindowsStore",
            "AutoDownload",
            if on { 4 } else { 2 },
        );
        let _ = reg_dword(
            r"HKLM\SOFTWARE\Policies\Google\Update",
            "UpdateDefault",
            on as u32,
        );
        let _ = reg_dword(
            r"HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate",
            "UpdateDefault",
            on as u32,
        );
        Ok(())
    }

    /// winget lives in a versioned WindowsApps folder; SYSTEM has no PATH entry for it.
    pub fn winget_path() -> String {
        let base = std::path::Path::new(r"C:\Program Files\WindowsApps");
        if let Ok(rd) = std::fs::read_dir(base) {
            let mut dirs: Vec<_> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        n.starts_with("Microsoft.DesktopAppInstaller_") && n.contains("_x64__")
                    })
                })
                .collect();
            dirs.sort();
            if let Some(d) = dirs.last() {
                return d.join("winget.exe").to_string_lossy().into_owned();
            }
        }
        "winget".into()
    }

    const SEARCH: &str = "$s=New-Object -ComObject Microsoft.Update.Session; \
        $r=$s.CreateUpdateSearcher().Search(\"IsInstalled=0 and IsHidden=0 and Type='Software'\"); \
        Write-Output (\"COUNT \" + $r.Updates.Count)";

    const INSTALL: &str = "$ErrorActionPreference='Stop'; $s=New-Object -ComObject Microsoft.Update.Session; \
        $r=$s.CreateUpdateSearcher().Search(\"IsInstalled=0 and IsHidden=0 and Type='Software'\"); \
        if($r.Updates.Count -eq 0){ Write-Output 'INSTALLED 0'; exit 0 }; \
        $c=New-Object -ComObject Microsoft.Update.UpdateColl; \
        foreach($u in $r.Updates){ if(-not $u.EulaAccepted){ $u.AcceptEula() }; [void]$c.Add($u) }; \
        $d=$s.CreateUpdateDownloader(); $d.Updates=$c; [void]$d.Download(); \
        $i=$s.CreateUpdateInstaller(); $i.Updates=$c; $res=$i.Install(); \
        Write-Output (\"INSTALLED \" + $c.Count + \" REBOOT \" + $res.RebootRequired)";

    fn count_after(text: &str, tag: &str) -> Option<u32> {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(tag))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
    }

    pub fn check() -> Result<Pending> {
        let (_, out) = sh::script(SEARCH)?;
        let os = count_after(&out, "COUNT ").ok_or_else(|| {
            anyhow::anyhow!("Windows Update javob bermadi: {}", sh::tail(&out, 2))
        })?;
        let (_, wg) = sh::run(
            &winget_path(),
            &[
                "upgrade",
                "--accept-source-agreements",
                "--disable-interactivity",
            ],
        )
        .unwrap_or((false, String::new()));
        // "N upgrades available." (English) — fall back to counting table rows.
        let apps = wg
            .lines()
            .find(|l| l.contains("upgrades available") || l.contains("upgrade available"))
            .and_then(|l| l.split_whitespace().next()?.parse().ok())
            .unwrap_or(0);
        Ok(Pending { os, apps })
    }

    pub fn install_os() -> Result<String> {
        let (ok, out) = sh::script(INSTALL)?;
        match count_after(&out, "INSTALLED ") {
            Some(n) => Ok(format!("Windows: {n} ta yangilanish")),
            None if !ok => anyhow::bail!("Windows Update xatosi: {}", sh::tail(&out, 3)),
            None => Ok("Windows: tayyor".into()),
        }
    }

    pub fn policy_touched() -> crate::sh::Touched {
        let registry = [
            (WU, "NoAutoUpdate"),
            (WU, "AUOptions"),
            (
                r"HKLM\SOFTWARE\Policies\Microsoft\WindowsStore",
                "AutoDownload",
            ),
            (r"HKLM\SOFTWARE\Policies\Google\Update", "UpdateDefault"),
            (
                r"HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate",
                "UpdateDefault",
            ),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        crate::sh::Touched {
            files: vec![],
            registry,
        }
    }

    pub fn reboot_required() -> bool {
        sh::script("Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate\\Auto Update\\RebootRequired'")
            .map(|(_, o)| o.contains("True"))
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    const AUTO: &str = "/etc/apt/apt.conf.d/20auto-upgrades";

    pub fn set_policy(p: UpdatePolicy) -> Result<()> {
        let (lists, unattended) = match p.os {
            OsUpdateMode::Auto => (1, 1),
            OsUpdateMode::Manual => (1, 0),
            OsUpdateMode::Disabled => (0, 0),
        };
        std::fs::write(
            AUTO,
            format!("// Managed by Vimora\nAPT::Periodic::Update-Package-Lists \"{lists}\";\nAPT::Periodic::Unattended-Upgrade \"{unattended}\";\n"),
        )?;
        let _ = sh::run(
            "snap",
            &["refresh", if p.apps_auto { "--unhold" } else { "--hold" }],
        );
        Ok(())
    }

    pub fn check() -> Result<Pending> {
        let _ = sh::run("apt-get", &["update", "-qq"]);
        let (_, sim) = sh::run("apt-get", &["-s", "upgrade"])?;
        let os = sim.lines().filter(|l| l.starts_with("Inst ")).count() as u32;
        let (_, snaps) = sh::run("snap", &["refresh", "--list"]).unwrap_or((false, String::new()));
        let apps = snaps
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty() && !l.contains("All snaps up to date"))
            .count() as u32;
        Ok(Pending { os, apps })
    }

    pub fn install_os() -> Result<String> {
        sh::run_ok(
            "sh",
            &["-c", "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get -y -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold upgrade"],
        )?;
        Ok("apt: yangilandi".into())
    }

    pub fn policy_touched() -> crate::sh::Touched {
        crate::sh::Touched {
            files: vec![AUTO.into()],
            registry: vec![],
        }
    }

    pub fn reboot_required() -> bool {
        std::path::Path::new("/var/run/reboot-required").exists()
    }
}
