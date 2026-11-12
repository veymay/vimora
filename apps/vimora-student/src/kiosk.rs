//! While the lock screen is up, the PC is "frozen": the service (SYSTEM) switches off the system
//! ways out that no keyboard hook can stop.
//!
//! Windows: Ctrl+Alt+Del is handled by Windows itself, so it can't be swallowed — but every
//! choice on that screen can be turned off by policy (Task Manager, lock, change password,
//! sign out, switch user, shut down). What's left is a screen with only "Cancel". The policies
//! are applied to every signed-in user and taken back exactly as they were on unlock: values the
//! administrator had set before are never touched.
//!
//! Other systems: nothing to do here (the lock screen's own input grab covers them).

use anyhow::Result;

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::sh;
    use std::path::PathBuf;

    const SYSTEM: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\System";
    const EXPLORER: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer";
    /// (per-user key, value) switched to 1 while locked.
    const USER: &[(&str, &str)] = &[
        (SYSTEM, "DisableTaskMgr"),
        (SYSTEM, "DisableLockWorkstation"),
        (SYSTEM, "DisableChangePassword"),
        (EXPLORER, "NoLogoff"),
        (EXPLORER, "NoClose"),
    ];
    const MACHINE: (&str, &str) = (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
        "HideFastUserSwitching",
    );

    /// Values Vimora created (full key, name), so unlocking removes only those.
    fn state_path() -> PathBuf {
        PathBuf::from(std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into()))
            .join("Vimora")
            .join("kiosk.json")
    }

    fn load() -> Vec<(String, String)> {
        std::fs::read_to_string(state_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(v: &[(String, String)]) {
        let p = state_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if v.is_empty() {
            let _ = std::fs::remove_file(p);
        } else if let Ok(j) = serde_json::to_string(v) {
            let _ = std::fs::write(p, j);
        }
    }

    /// Signed-in users' hives (HKU\S-1-5-21-…, without the _Classes twins).
    fn user_hives() -> Vec<String> {
        let Ok((_, out)) = sh::run("reg", &["query", "HKU"]) else {
            return vec![];
        };
        out.lines()
            .map(str::trim)
            .filter(|l| l.starts_with(r"HKEY_USERS\S-1-5-21-") && !l.ends_with("_Classes"))
            .map(|l| l.replacen("HKEY_USERS", "HKU", 1))
            .collect()
    }

    fn exists(key: &str, name: &str) -> bool {
        sh::run("reg", &["query", key, "/v", name]).is_ok_and(|(ok, _)| ok)
    }

    pub fn set(on: bool) -> Result<()> {
        let mut ours = load();
        if on {
            let mut targets: Vec<(String, String)> = user_hives()
                .iter()
                .flat_map(|h| {
                    USER.iter()
                        .map(move |(k, n)| (format!(r"{h}\{k}"), n.to_string()))
                })
                .collect();
            targets.push((MACHINE.0.to_string(), MACHINE.1.to_string()));
            for (key, name) in targets {
                let mine = ours.iter().any(|(k, n)| *k == key && *n == name);
                if !mine && exists(&key, &name) {
                    continue; // set by the administrator: leave it alone
                }
                sh::run_ok(
                    "reg",
                    &["add", &key, "/v", &name, "/t", "REG_DWORD", "/d", "1", "/f"],
                )?;
                if !mine {
                    ours.push((key, name));
                }
            }
        } else {
            for (key, name) in &ours {
                let _ = sh::run("reg", &["delete", key, "/v", name, "/f"]);
            }
            ours.clear();
        }
        save(&ours);
        touchpad(on);
        Ok(())
    }

    /// Touchpads that Vimora switched off (PnP instance ids), so unlocking (or the next service
    /// start after a crash) switches exactly those on again.
    fn touchpad_state() -> PathBuf {
        state_path().with_file_name("kiosk-touchpad.txt")
    }

    /// Touchpad gestures (three/four-finger swipes: desktop, task view, app switching) are
    /// handled by Windows itself, past every hook — so the touchpad is off while the lock screen
    /// is up (the pointer is blocked then anyway). Touchpads are found by their standard HID
    /// usage (digitizer / touch pad), whatever the Windows language.
    fn touchpad(on: bool) {
        let path = touchpad_state();
        if on {
            const SCRIPT: &str = r#"
$ids = Get-CimInstance Win32_PnPEntity | Where-Object { $_.Status -eq 'OK' -and ($_.CompatibleID -contains 'HID_DEVICE_UP:000D_U:0005' -or $_.HardwareID -contains 'HID_DEVICE_UP:000D_U:0005') } | ForEach-Object { $_.PNPDeviceID }
foreach ($id in $ids) { try { Disable-PnpDevice -InstanceId $id -Confirm:$false -ErrorAction Stop; $id } catch {} }
"#;
            match sh::script(SCRIPT) {
                Ok((_, out)) => {
                    let mut ids: Vec<String> = std::fs::read_to_string(&path)
                        .unwrap_or_default()
                        .lines()
                        .map(str::to_string)
                        .collect();
                    ids.extend(
                        out.lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty())
                            .map(str::to_string),
                    );
                    ids.sort();
                    ids.dedup();
                    if !ids.is_empty() {
                        let _ = std::fs::write(&path, ids.join("\n"));
                    }
                }
                Err(e) => tracing::debug!("touchpad off: {e:#}"),
            }
        } else {
            let Ok(text) = std::fs::read_to_string(&path) else {
                return;
            };
            let ids: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            let list = ids
                .iter()
                .map(|i| format!("'{}'", i.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            let script = format!(
                "foreach ($id in @({list})) {{ try {{ Enable-PnpDevice -InstanceId $id -Confirm:$false -ErrorAction Stop }} catch {{}} }}"
            );
            match sh::script(&script) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&path);
                }
                Err(e) => tracing::warn!("touchpad on: {e:#}"),
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn set(_on: bool) -> Result<()> {
        Ok(())
    }
}

/// Freeze (`true`) or release (`false`) the system ways out of the lock screen.
pub fn set(on: bool) -> Result<()> {
    imp::set(on)
}
