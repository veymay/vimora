//! `vimora-student purge`: run by the uninstaller (as administrator / root) so that nothing of
//! Vimora stays on a student PC. Settings the service changed are given back (internet,
//! devices, update policies, firewall, lock), the PC's pairing and every user's Vimora files
//! are deleted.
//!
//! A frozen PC (UWF / Shadow Defender) would bring everything back on the next restart. Then
//! the freeze is switched off, a marker is kept, and the uninstaller stops with exit code 3;
//! after the restart the service finds the marker and runs the uninstaller itself.

use anyhow::Result;
use std::path::{Path, PathBuf};
use vimora_core::{AccessPolicy, DevicePolicy, FreezeAction, NetPolicy, PrivCommand};

/// Exit code: frozen, finishes after a restart.
pub const DEFERRED: i32 = 3;

/// Files the student agent keeps in a user's `vimora` config folder. The teacher keeps its
/// own files there too (`server.json`, `identity.key`): those stay on a teacher's PC.
const CLIENT_FILES: &[&str] = &[
    "student.json",
    "paired-teacher",
    "released",
    "last-teacher",
    "sound-off",
    "overlay-plan-2",
    "lock-state.json",
    "app-rules.json",
];

/// Where the marker "uninstall after the restart" is kept.
pub fn marker() -> PathBuf {
    crate::service::config_path().with_file_name("uninstall-pending")
}

pub fn run() -> Result<()> {
    let engine = crate::privileged::Engine::new();
    let shield = engine.shield();
    if cfg!(windows) && shield.protected_now() {
        let _ = engine.execute(PrivCommand::Freeze {
            action: FreezeAction::Thaw,
            restart: false,
        });
        let m = marker();
        if let Some(d) = m.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        std::fs::write(&m, "1")?;
        shield.commit_files(&[m.as_path()]);
        println!(
            "Muzlatish o'chirildi. Kompyuter qayta yoqilgach Vimora o'zi butunlay o'chiriladi."
        );
        std::process::exit(DEFERRED);
    }

    // Give back everything the service changed.
    for cmd in [
        PrivCommand::SetNetPolicy(NetPolicy::default()),
        PrivCommand::SetAccessPolicy(AccessPolicy::default()),
        PrivCommand::SetDevicePolicy(DevicePolicy::default()),
        PrivCommand::Freeze {
            action: FreezeAction::Thaw,
            restart: false,
        },
    ] {
        if let Err(e) = engine.execute(cmd) {
            tracing::debug!("purge: {e:#}");
        }
    }
    let _ = crate::kiosk::set(false);
    for (key, value) in crate::updates::policy_touched().registry {
        delete_reg_value(&key, &value);
    }
    for (key, value) in crate::firewall::touched().registry {
        delete_reg_value(&key, &value);
    }

    // Pairing, freeze state, updates: the service's folder.
    if let Some(dir) = crate::service::config_path().parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
    #[cfg(not(windows))]
    for d in ["/var/cache/vimora", "/var/lib/vimora"] {
        let _ = std::fs::remove_dir_all(d);
    }
    // Every user's Vimora files.
    for home in homes() {
        clean_config(&config_dir_of(&home));
        let _ = std::fs::remove_dir_all(home.join("Downloads").join("Vimora"));
    }
    println!("Vimora ma'lumotlari o'chirildi.");
    Ok(())
}

/// A user's `vimora` config folder: the agent's files go; the teacher's stay.
fn clean_config(dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    if dir.join("server.json").exists() || dir.join("identity.key").exists() {
        for f in CLIENT_FILES {
            let _ = std::fs::remove_file(dir.join(f));
        }
    } else {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(windows)]
fn config_dir_of(home: &Path) -> PathBuf {
    home.join("AppData").join("Roaming").join("vimora")
}

#[cfg(not(windows))]
fn config_dir_of(home: &Path) -> PathBuf {
    home.join(".config").join("vimora")
}

/// Home folders of all users (and of the system account the service runs as).
fn homes() -> Vec<PathBuf> {
    let mut v = vec![];
    #[cfg(windows)]
    {
        let users = std::env::var_os("PUBLIC")
            .map(PathBuf::from)
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(r"C:\Users"));
        if let Ok(rd) = std::fs::read_dir(users) {
            v.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        let win = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
        v.push(
            PathBuf::from(win)
                .join("System32")
                .join("config")
                .join("systemprofile"),
        );
    }
    #[cfg(not(windows))]
    {
        if let Ok(rd) = std::fs::read_dir("/home") {
            v.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        v.push(PathBuf::from("/root"));
    }
    v
}

#[cfg(windows)]
fn delete_reg_value(key: &str, value: &str) {
    let _ = crate::sh::run("reg", &["delete", key, "/v", value, "/f"]);
}

#[cfg(not(windows))]
fn delete_reg_value(_key: &str, _value: &str) {}

/// Service start: an uninstall that waited for the freeze to be switched off finishes now.
pub fn finish_pending() {
    if !marker().exists() {
        return;
    }
    if crate::privileged::Engine::new().shield().protected_now() {
        return; // still frozen: wait for the next restart
    }
    #[cfg(windows)]
    if let Some(un) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("uninstall.exe")))
        .filter(|p| p.exists())
    {
        tracing::info!("finishing the uninstall that waited for the freeze to end");
        let _ = std::process::Command::new(un)
            .args(["/S", "/BYTEACHER"])
            .spawn();
        return;
    }
    let _ = std::fs::remove_file(marker());
}
