//! Windows backend: Unified Write Filter (`uwfmgr.exe`).
//!
//! Configuration applied when arming:
//! * disk-backed overlay (not RAM) sized from free space, so long lessons can't exhaust memory;
//! * warning / critical thresholds at 75 % / 90 % of the overlay;
//! * protection of the system volume;
//! * exclusions: Vimora's data, Windows Defender signatures and settings (so antivirus stays
//!   current), and the domain machine-account secret (so the domain trust doesn't break).

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
    })
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
    fn config(&self) -> Option<Config> {
        let (_, out) = sh::run("uwfmgr.exe", &["get-config"]).ok()?;
        parse_config(&out)
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
        Ok(())
    }

    fn add_exclusions(&self, persistent: &[String]) {
        let mut files = vec![
            data_dir().to_string_lossy().into_owned(),
            r"C:\ProgramData\Microsoft\Windows Defender".to_string(),
            r"C:\Program Files\Windows Defender".to_string(),
        ];
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
            return Probe {
                state: FreezeState::Unsupported,
                usage: None,
                note: Some(
                    "UWF yoqilmagan yoki bu nashrda yo'q (Enterprise / Education kerak)".into(),
                ),
            };
        };
        let usage = if cfg.filter_now {
            let used = sh::run("uwfmgr.exe", &["overlay", "get-consumption"])
                .ok()
                .and_then(|(_, o)| first_number(&o));
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
            // The optional feature exists on Enterprise / Education / IoT only.
            let (ok, out) = sh::run(
                "dism.exe",
                &[
                    "/online",
                    "/enable-feature",
                    "/featurename:Client-UnifiedWriteFilter",
                    "/all",
                    "/norestart",
                ],
            )?;
            if ok {
                return Ok("UWF komponenti o'rnatildi — kompyuterni qayta yuklang va Shadow ni yana yoqing".into());
            }
            bail!(
                "bu Windows nashrida UWF yo'q (Enterprise/Education kerak): {}",
                sh::tail(&out, 2)
            );
        };
        // The overlay can only be reconfigured while the filter is off in this session.
        if !cfg.filter_now {
            self.configure_overlay()?;
        }
        uwf(&["filter", "enable"])?;
        uwf(&["volume", "protect", &system_drive()])?;
        self.add_exclusions(persistent);
        Ok("Shadow rejim qayta yuklangandan keyin yoqiladi".into())
    }

    fn disable(&self) -> Result<String> {
        if self.config().is_none() {
            bail!("UWF o'rnatilmagan");
        }
        uwf(&["filter", "disable"])?;
        Ok("Shadow rejim qayta yuklangandan keyin o'chadi".into())
    }

    fn commit_file(&self, path: &Path) -> Result<()> {
        uwf(&["file", "commit", &path.to_string_lossy()])?;
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
                overlay_max_mb: Some(8192)
            }
        );
        assert_eq!(state_of(&c), FreezeState::ThawPending);
        assert_eq!(first_number("Overlay consumption:  123 MB"), Some(123));
    }
}
