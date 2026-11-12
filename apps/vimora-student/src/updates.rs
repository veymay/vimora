//! OS and application updates, controlled from the teacher.
//!
//! * Windows: Windows Update through its COM API (PowerShell), policies in
//!   `HKLM\SOFTWARE\Policies`, apps through `winget` plus Store/Chrome/Edge update policies.
//! * Ubuntu: apt (`unattended-upgrades` policy) and snap.

use crate::sh;
use anyhow::Result;
use vimora_core::{OsUpdateMode, UpdateItem, UpdatePolicy, UpdatesInfo};

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

/// How many applications have an update waiting (winget / snap).
pub fn apps_pending() -> Result<u32> {
    imp::apps_pending()
}

/// The system (name, build) and every pending update with its versions (drivers included).
pub fn details() -> Result<UpdatesInfo> {
    imp::details()
}

/// Installs only these pending updates (ids from [`details`]).
pub fn install_ids(ids: &[String]) -> Result<String> {
    // Only ids made of letters, digits, '-', '.', '+', ':' (GUIDs, package names).
    let ids: Vec<&String> = ids
        .iter()
        .filter(|i| {
            !i.is_empty()
                && i.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-.+:_".contains(c))
        })
        .collect();
    if ids.is_empty() {
        anyhow::bail!("yangilanish tanlanmagan");
    }
    imp::install_ids(&ids)
}

/// Parses the tab-separated report of the details scripts.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_details(text: &str) -> UpdatesInfo {
    let mut info = UpdatesInfo::default();
    for line in text.lines() {
        let f: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        match f.first().copied() {
            Some("OS") if f.len() >= 3 => {
                info.os_name = f[1].trim().to_string();
                info.os_build = f[2].trim().to_string();
            }
            Some("ITEM") if f.len() >= 7 => info.items.push(UpdateItem {
                id: f[1].trim().to_string(),
                driver: f[2].trim() == "1",
                title: f[3].trim().to_string(),
                current: f[4].trim().to_string(),
                new: f[5].trim().to_string(),
                size_mb: f[6].trim().parse().unwrap_or(0),
            }),
            _ => {}
        }
    }
    info
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

    pub fn apps_pending() -> Result<u32> {
        let (_, wg) = sh::run(
            &winget_path(),
            &[
                "upgrade",
                "--accept-source-agreements",
                "--disable-interactivity",
            ],
        )
        .unwrap_or((false, String::new()));
        // "N upgrades available." (English).
        Ok(wg
            .lines()
            .find(|l| l.contains("upgrades available") || l.contains("upgrade available"))
            .and_then(|l| l.split_whitespace().next()?.parse().ok())
            .unwrap_or(0))
    }

    /// One PowerShell run: the system, installed driver versions, every pending update.
    const DETAILS: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$cv=Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$os=((Get-CimInstance Win32_OperatingSystem).Caption -replace '^Microsoft ','')
$build="$($cv.CurrentBuild).$($cv.UBR)"
"OS`t$os $($cv.DisplayVersion)`t$build"
$drv=@{}
foreach($d in Get-CimInstance Win32_PnPSignedDriver){ if($d.HardWareID -and $d.DriverVersion){ $drv[([string]$d.HardWareID).ToUpper()]=$d.DriverVersion } }
$s=New-Object -ComObject Microsoft.Update.Session
$r=$s.CreateUpdateSearcher().Search("IsInstalled=0 and IsHidden=0")
foreach($u in $r.Updates){
  $isd=($u.Type -eq 2); $cur=''; $new=''
  if($isd){
    $h=[string]$u.DriverHardwareID; if($h){ $cur=$drv[$h.ToUpper()] }
    if($u.DriverVerDate){ $new=$u.DriverVerDate.ToString('yyyy-MM-dd') }
    if($u.Title -match '(\d+(\.\d+){2,})\s*$'){ $new=$matches[1] }
  } else {
    $cur=$build; if($u.Title -match '(KB\d+)'){ $new=$matches[1] }
  }
  $mb=[int]($u.MaxDownloadSize/1MB)
  "ITEM`t$($u.Identity.UpdateID)`t$([int]$isd)`t$($u.Title -replace "`t",' ')`t$cur`t$new`t$mb"
}
"#;

    pub fn details() -> Result<UpdatesInfo> {
        let (_, out) = sh::script(DETAILS)?;
        let info = super::parse_details(&out);
        if info.os_name.is_empty() {
            anyhow::bail!("Windows Update javob bermadi: {}", sh::tail(&out, 2));
        }
        Ok(info)
    }

    pub fn install_ids(ids: &[&String]) -> Result<String> {
        let list = ids
            .iter()
            .map(|i| format!("'{i}'"))
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            "$ErrorActionPreference='Stop'; $ids=@({list}); $s=New-Object -ComObject Microsoft.Update.Session; \
             $r=$s.CreateUpdateSearcher().Search(\"IsInstalled=0 and IsHidden=0\"); \
             $c=New-Object -ComObject Microsoft.Update.UpdateColl; \
             foreach($u in $r.Updates){{ if($ids -contains $u.Identity.UpdateID){{ if(-not $u.EulaAccepted){{ $u.AcceptEula() }}; [void]$c.Add($u) }} }}; \
             if($c.Count -eq 0){{ Write-Output 'INSTALLED 0'; exit 0 }}; \
             $d=$s.CreateUpdateDownloader(); $d.Updates=$c; [void]$d.Download(); \
             $i=$s.CreateUpdateInstaller(); $i.Updates=$c; $res=$i.Install(); \
             Write-Output (\"INSTALLED \" + $c.Count + \" REBOOT \" + $res.RebootRequired)"
        );
        let (ok, out) = sh::script(&script)?;
        match count_after(&out, "INSTALLED ") {
            Some(n) => Ok(format!("Windows: {n} ta yangilanish o'rnatildi")),
            None if !ok => anyhow::bail!("Windows Update xatosi: {}", sh::tail(&out, 3)),
            None => Ok("Windows: tayyor".into()),
        }
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

    pub fn apps_pending() -> Result<u32> {
        let (_, snaps) = sh::run("snap", &["refresh", "--list"]).unwrap_or((false, String::new()));
        Ok(snaps
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty() && !l.contains("All snaps up to date"))
            .count() as u32)
    }

    pub fn details() -> Result<UpdatesInfo> {
        let os_name = std::fs::read_to_string("/etc/os-release")
            .unwrap_or_default()
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_else(|| "Linux".into());
        let os_build = sh::run("uname", &["-r"])
            .map(|(_, o)| o.trim().to_string())
            .unwrap_or_default();
        let _ = sh::run("apt-get", &["update", "-qq"]);
        let (_, list) = sh::run("apt", &["list", "--upgradable"]).unwrap_or((false, String::new()));
        // "pkg/suite 1.2-3 amd64 [upgradable from: 1.2-1]"
        let items = list
            .lines()
            .filter_map(|l| {
                let (pkg, rest) = l.split_once('/')?;
                let new = rest.split_whitespace().nth(1)?.to_string();
                let current = rest
                    .split("from: ")
                    .nth(1)
                    .map(|c| c.trim_end_matches(']').to_string())
                    .unwrap_or_default();
                Some(UpdateItem {
                    id: pkg.to_string(),
                    title: pkg.to_string(),
                    driver: false,
                    current,
                    new,
                    size_mb: 0,
                })
            })
            .collect();
        Ok(UpdatesInfo {
            os_name,
            os_build,
            items,
        })
    }

    pub fn install_ids(ids: &[&String]) -> Result<String> {
        let mut args = vec![
            "install",
            "--only-upgrade",
            "-y",
            "-o",
            "DPkg::Lock::Timeout=300",
        ];
        args.extend(ids.iter().map(|s| s.as_str()));
        sh::run_ok("apt-get", &args)?;
        Ok(format!("{} ta paket yangilandi", ids.len()))
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

#[cfg(test)]
mod tests {
    #[test]
    fn parses_update_report() {
        let text = "OS\tWindows 11 Education 24H2\t26100.2033\r\n\
                    ITEM\tabc-1\t0\t2026-11 Cumulative Update (KB5046617)\t26100.2033\tKB5046617\t612\r\n\
                    ITEM\tdef-2\t1\tIntel - Display - 31.0.101.5590\t31.0.101.4502\t31.0.101.5590\t412\n\
                    noise\n";
        let info = super::parse_details(text);
        assert_eq!(info.os_name, "Windows 11 Education 24H2");
        assert_eq!(info.os_build, "26100.2033");
        assert_eq!(info.items.len(), 2);
        assert!(!info.items[0].driver && info.items[1].driver);
        assert_eq!(info.items[1].current, "31.0.101.4502");
        assert_eq!(info.items[1].new, "31.0.101.5590");
        assert_eq!(info.items[0].size_mb, 612);
    }
}
