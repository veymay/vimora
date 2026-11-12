//! Windows backend: Unified Write Filter (`uwfmgr.exe`).
//!
//! Fully automatic on Windows 10/11 Education / Enterprise: switching "Muzlatish" on from the
//! teacher installs the optional UWF component if it is missing (DISM) and restarts; the next
//! boot arms the filter and restarts once more (see `Shield::on_boot`). From then on every
//! change — settings, installed programs, downloaded files — is gone after a restart.
//!
//! Configuration applied when arming:
//! * disk-backed overlay (not RAM) sized from free space, with free-space passthrough, so long
//!   lessons can't exhaust memory;
//! * warning / critical thresholds at 75 % / 90 % of the overlay;
//! * protection of every fixed volume (C:, D:, …), so files downloaded anywhere are rolled back;
//! * exclusions: Vimora's data and each user's teacher pairing, Windows Defender signatures and
//!   settings (so antivirus stays current), the domain machine-account secret, and the folders
//!   the teacher marked as persistent;
//! * what UWF needs to work well: fast startup (hibernation) and scheduled defragmentation off.

use super::{overlay_size_mb, Backend, Probe};
use crate::sh;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use vimora_core::{FreezeState, ShieldUsage};

#[derive(Default)]
pub struct Uwf;

/// Parsed `uwfmgr get-config` output.
#[derive(Debug, Default, PartialEq)]
pub struct Config {
    pub filter_now: bool,
    pub filter_next: bool,
    pub servicing_now: bool,
    pub servicing_next: bool,
    pub overlay_max_mb: Option<u32>,
    /// Overlay in use (MB), when known.
    pub overlay_used_mb: Option<u32>,
}

/// `uwfmgr get-config` prints a "Current Session" block followed by a "Next Session" block.
pub fn parse_config(text: &str) -> Option<Config> {
    let lower = text.to_lowercase();
    let split = lower.find("next session").unwrap_or(lower.len());
    let (now, next) = lower.split_at(split);
    let flag = |block: &str, key: &str| -> Option<bool> {
        block
            .lines()
            .find(|l| l.contains(key))
            .map(|l| l.split(':').nth(1).unwrap_or("").trim().starts_with("on"))
    };
    let filter_now = flag(now, "filter state")?;
    let max = now
        .lines()
        .find(|l| l.contains("maximum size"))
        .and_then(|l| first_number(l.split(':').nth(1)?));
    Some(Config {
        filter_now,
        filter_next: flag(next, "filter state").unwrap_or(filter_now),
        servicing_now: flag(now, "servicing state").unwrap_or(false),
        servicing_next: flag(next, "servicing state").unwrap_or(false),
        overlay_max_mb: max,
        overlay_used_mb: None,
    })
}

/// UWF state from WMI (`root\\standardcimv2\\embedded`). None when UWF isn't active on this
/// boot (not installed, or installed and waiting for a restart).
fn wmi_config() -> Option<Config> {
    const SCRIPT: &str = r#"
$ns='root/standardcimv2/embedded'
$f=Get-CimInstance -Namespace $ns -ClassName UWF_Filter -ErrorAction Stop
$o=@(Get-CimInstance -Namespace $ns -ClassName UWF_OverlayConfig -ErrorAction SilentlyContinue)
$s=@(Get-CimInstance -Namespace $ns -ClassName UWF_Servicing -ErrorAction SilentlyContinue)
$ov=Get-CimInstance -Namespace $ns -ClassName UWF_Overlay -ErrorAction SilentlyContinue
@{now=[bool]$f.CurrentEnabled; next=[bool]$f.NextEnabled;
  max=[int](($o | ? { $_.CurrentSession }) | Select-Object -First 1).MaximumSize;
  svcNow=[bool](($s | ? { $_.CurrentSession }) | Select-Object -First 1).ServicingEnabled;
  svcNext=[bool](($s | ? { -not $_.CurrentSession }) | Select-Object -First 1).ServicingEnabled;
  used=[int]($ov | Select-Object -First 1).OverlayConsumption} | ConvertTo-Json -Compress
"#;
    let (ok, out) = sh::script(SCRIPT).ok()?;
    if !ok {
        return None;
    }
    let v: serde_json::Value = out
        .lines()
        .find(|l| l.trim_start().starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())?;
    let b = |k: &str| v.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
    let n = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_u64())
            .filter(|n| *n > 0)
            .map(|n| n as u32)
    };
    Some(Config {
        filter_now: b("now"),
        filter_next: b("next"),
        servicing_now: b("svcNow"),
        servicing_next: b("svcNext"),
        overlay_max_mb: n("max"),
        overlay_used_mb: n("used"),
    })
}

/// Whether the UWF optional feature is turned on (maybe still waiting for a restart).
fn feature_state() -> Option<String> {
    let (_, out) = sh::script(
        "(Get-WindowsOptionalFeature -Online -FeatureName Student-UnifiedWriteFilter -ErrorAction SilentlyContinue).State",
    )
    .ok()?;
    let s = out.trim().to_string();
    (!s.is_empty()).then_some(s)
}

pub fn first_number(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn state_of(c: &Config) -> FreezeState {
    if c.servicing_now || c.servicing_next {
        return FreezeState::Maintenance;
    }
    match (c.filter_now, c.filter_next) {
        (true, true) => FreezeState::Frozen,
        (true, false) => FreezeState::ThawPending,
        (false, true) => FreezeState::FreezePending,
        (false, false) => FreezeState::Thawed,
    }
}

fn uwf(args: &[&str]) -> Result<String> {
    sh::run_ok("uwfmgr.exe", args)
}

fn system_drive() -> String {
    std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
}

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()))
        .join("Vimora")
}

/// Fixed (internal) drives, "C:", "D:", … — the system drive always first.
fn fixed_drives() -> Vec<String> {
    let sys = system_drive();
    #[allow(unused_mut)]
    let mut out = vec![sys.clone()];
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
        const DRIVE_FIXED: u32 = 3;
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let d = format!("{letter}:");
            let root: Vec<u16> = format!("{d}\\").encode_utf16().chain(Some(0)).collect();
            if unsafe { GetDriveTypeW(root.as_ptr()) } == DRIVE_FIXED
                && !d.eq_ignore_ascii_case(&sys)
            {
                out.push(d);
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn free_mb(_drive: &str) -> u64 {
    0
}

#[cfg(windows)]
fn free_mb(drive: &str) -> u64 {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let root: Vec<u16> = format!("{drive}\\").encode_utf16().chain(Some(0)).collect();
    let mut free = 0u64;
    unsafe {
        if GetDiskFreeSpaceExW(
            root.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
        {
            return 0;
        }
    }
    free / (1024 * 1024)
}

impl Uwf {
    /// UWF's state. Read through WMI first: it doesn't depend on the Windows display
    /// language or on how uwfmgr prints; `uwfmgr get-config` is the fallback.
    fn config(&self) -> Option<Config> {
        wmi_config().or_else(|| {
            let (_, out) = sh::run("uwfmgr.exe", &["get-config"]).ok()?;
            parse_config(&out)
        })
    }

    /// Settings UWF needs: no hibernation / fast startup (a resumed session would skip the
    /// rollback) and no scheduled defragmentation (it would fill the overlay).
    fn prepare_windows(&self) {
        for (prog, args) in [
            ("powercfg.exe", &["/h", "off"][..]),
            (
                "schtasks.exe",
                &[
                    "/Change",
                    "/TN",
                    r"\Microsoft\Windows\Defrag\ScheduledDefrag",
                    "/Disable",
                ][..],
            ),
        ] {
            if let Err(e) = sh::run_ok(prog, args) {
                tracing::debug!("uwf prepare {prog}: {e:#}");
            }
        }
    }

    fn configure_overlay(&self) -> Result<()> {
        let size = overlay_size_mb(free_mb(&system_drive()));
        uwf(&["overlay", "set-type", "disk"])?;
        uwf(&["overlay", "set-size", &size.to_string()])?;
        uwf(&[
            "overlay",
            "set-warningthreshold",
            &(size * 75 / 100).to_string(),
        ])?;
        uwf(&[
            "overlay",
            "set-criticalthreshold",
            &(size * 90 / 100).to_string(),
        ])?;
        // Writes into free space where possible: the overlay fills up far more slowly.
        if let Err(e) = uwf(&["overlay", "set-passthrough", "on"]) {
            tracing::debug!("uwf passthrough: {e:#}");
        }
        Ok(())
    }

    fn add_exclusions(&self, persistent: &[String]) {
        let mut files = vec![
            data_dir().to_string_lossy().into_owned(),
            r"C:\ProgramData\Microsoft\Windows Defender".to_string(),
            r"C:\Program Files\Windows Defender".to_string(),
        ];
        // Vimora itself: an update (new program files) is never rolled back.
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        {
            files.push(dir);
        }
        // Each user's Vimora settings (%APPDATA%\vimora: pairing, program lists, sound) and the
        // files the teacher sent (Downloads\Vimora).
        if let Ok(users) = std::fs::read_dir(format!("{}\\Users", system_drive())) {
            for u in users.flatten() {
                let roaming = u.path().join("AppData").join("Roaming");
                if roaming.is_dir() {
                    files.push(roaming.join("vimora").to_string_lossy().into_owned());
                    files.push(
                        u.path()
                            .join("Downloads")
                            .join("Vimora")
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        files.extend(persistent.iter().cloned());
        files.push(r"C:\Windows\System32\drivers\etc\hosts".to_string());
        for f in &files {
            if !f.ends_with("hosts") {
                let _ = std::fs::create_dir_all(f);
            }
            if let Err(e) = uwf(&["file", "add-exclusion", f]) {
                tracing::debug!("uwf exclusion {f}: {e:#}");
            }
        }
        // Teacher-managed settings (all Vimora policies live under Policies) always persist.
        for k in [
            r"HKLM\SOFTWARE\Policies",
            r"HKLM\SOFTWARE\Microsoft\Windows Defender",
            r"HKLM\SECURITY\Policy\Secrets\$MACHINE.ACC",
            // Firewall rules (internet off, blocked ports) set from the teacher.
            r"HKLM\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy",
            // Device control (USB, printer, Bluetooth service start types) and Vimora's version.
            r"HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR",
            r"HKLM\SYSTEM\CurrentControlSet\Services\Spooler",
            r"HKLM\SYSTEM\CurrentControlSet\Services\bthserv",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Vimora",
        ] {
            if let Err(e) = uwf(&["registry", "add-exclusion", k]) {
                tracing::debug!("uwf exclusion {k}: {e:#}");
            }
        }
    }
}

impl Backend for Uwf {
    fn probe(&self) -> Probe {
        let Some(cfg) = self.config() else {
            // Not active on this boot: installed and waiting for a restart, or not installed.
            let pending = feature_state().is_some_and(|s| s.contains("Enable"));
            return Probe {
                state: if pending {
                    FreezeState::Thawed
                } else {
                    FreezeState::Unsupported
                },
                usage: None,
                note: Some(if pending {
                    "UWF o'rnatilgan — qayta yuklangach ishga tushadi (Muzlatishni yoqsangiz, Vimora o'zi qayta yuklaydi)".into()
                } else {
                    "UWF yo'q: Windows 10/11 Education yoki Enterprise kerak (Muzlatish yoqilsa, Vimora o'zi o'rnatadi)".into()
                }),
            };
        };
        let usage = if cfg.filter_now {
            let used = cfg.overlay_used_mb.or_else(|| {
                sh::run("uwfmgr.exe", &["overlay", "get-consumption"])
                    .ok()
                    .and_then(|(_, o)| first_number(&o))
            });
            used.zip(cfg.overlay_max_mb)
                .map(|(used_mb, max_mb)| ShieldUsage { used_mb, max_mb })
        } else {
            None
        };
        Probe {
            state: state_of(&cfg),
            usage,
            note: None,
        }
    }

    fn enable(&self, persistent: &[String]) -> Result<String> {
        let Some(cfg) = self.config() else {
            // The optional feature exists on Enterprise / Education / IoT only. Install it and
            // restart; the next boot finishes the job by itself (Shield::on_boot).
            let (code, out) = sh::run_code(
                "dism.exe",
                &[
                    "/online",
                    "/enable-feature",
                    "/featurename:Student-UnifiedWriteFilter",
                    "/all",
                    "/norestart",
                    "/quiet",
                ],
            )?;
            // 0 = done, 3010 = done, restart needed.
            if matches!(code, Some(0) | Some(3010)) {
                sh::restart_soon()?;
                return Ok("UWF komponenti o'rnatildi — kompyuter qayta yuklanadi, keyin muzlatish o'zi sozlanadi".into());
            }
            bail!(
                "bu Windows nashrida UWF yo'q (Education yoki Enterprise kerak): {}",
                sh::tail(&out, 2)
            );
        };
        self.prepare_windows();
        // The overlay can only be reconfigured while the filter is off in this session.
        if !cfg.filter_now {
            self.configure_overlay()?;
        }
        uwf(&["filter", "enable"])?;
        let drives = fixed_drives();
        for d in &drives {
            if let Err(e) = uwf(&["volume", "protect", d]) {
                // The system volume must be protected; others are best effort.
                if *d == system_drive() {
                    return Err(e);
                }
                tracing::warn!("uwf protect {d}: {e:#}");
            }
        }
        self.add_exclusions(persistent);
        Ok(format!(
            "Muzlatish (UWF) qayta yuklangandan keyin yoqiladi: {}",
            drives.join(", ")
        ))
    }

    fn disable(&self) -> Result<String> {
        if self.config().is_none() {
            bail!("UWF o'rnatilmagan");
        }
        uwf(&["filter", "disable"])?;
        Ok("Muzlatish (UWF) qayta yuklangandan keyin o'chadi".into())
    }

    fn commit_file(&self, path: &Path) -> Result<()> {
        uwf(&["file", "commit", &path.to_string_lossy()])?;
        Ok(())
    }

    /// Service start: (re)apply the exclusions, so PCs frozen by an older Vimora get the
    /// newer list too (exclusions take effect from the next restart).
    fn mount_persistent(&self, folders: &[String]) {
        if self.config().is_some() {
            self.add_exclusions(folders);
        }
    }

    /// UWF commits files one by one: a folder (e.g. Vimora's own after an update) is walked.
    fn commit_path(&self, path: &Path) -> Result<()> {
        if !path.is_dir() {
            return self.commit_file(path);
        }
        let (mut ok, mut failed) = (0, 0);
        let mut stack = vec![path.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if self.commit_file(&p).is_ok() {
                    ok += 1;
                } else {
                    failed += 1;
                }
            }
        }
        if ok == 0 && failed > 0 {
            bail!("{} dagi fayllarni saqlab bo'lmadi", path.display());
        }
        Ok(())
    }

    fn commit_registry(&self, key: &str, value: &str) -> Result<()> {
        uwf(&["registry", "commit", key, value])?;
        Ok(())
    }

    fn set_persistent(&self, new: &[String], old: &[String]) -> Result<String> {
        if self.config().is_none() {
            bail!("UWF o'rnatilmagan");
        }
        for f in old.iter().filter(|f| !new.contains(f)) {
            let _ = uwf(&["file", "remove-exclusion", f]);
        }
        for f in new.iter().filter(|f| !old.contains(f)) {
            std::fs::create_dir_all(f)?;
            uwf(&["file", "add-exclusion", f])?;
        }
        Ok(format!(
            "{} ta papka doim saqlanadi (qayta yuklangandan keyin kuchga kiradi)",
            new.len()
        ))
    }

    fn persist(&self, name: &str, contents: &str) -> Result<()> {
        // %ProgramData%\Vimora is excluded from the filter, so this survives restarts.
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(name), contents)?;
        Ok(())
    }

    fn read_persistent(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(data_dir().join(name)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Unified Write Filter Configuration for the Current Session\n\
        FILTER SETTINGS\n     Filter state:     ON\n     Pending commit:   N/A\n\
        SERVICING SETTINGS\n     Servicing state:  OFF\n\
        OVERLAY SETTINGS\n     Type:             DISK\n     Maximum size:     8192 MB\n\
        Unified Write Filter Configuration for the Next Session\n\
        FILTER SETTINGS\n     Filter state:     OFF\n\
        SERVICING SETTINGS\n     Servicing state:  OFF\n";

    #[test]
    fn parses_uwf_config() {
        let c = parse_config(SAMPLE).unwrap();
        assert_eq!(
            c,
            Config {
                filter_now: true,
                filter_next: false,
                servicing_now: false,
                servicing_next: false,
                overlay_max_mb: Some(8192),
                overlay_used_mb: None,
            }
        );
        assert_eq!(state_of(&c), FreezeState::ThawPending);
        assert_eq!(first_number("Overlay consumption:  123 MB"), Some(123));
    }
}
