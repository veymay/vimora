//! Where Vimora lives on a student computer.
//!
//! Windows: everything in one folder, `C:\Program Files (x86)\Vimora` (program, uninstaller,
//! icon and `Files`, the files the teacher sends). Up to 0.3.27 it was `C:\Program Files\Vimora`
//! with `vimora-client.exe`; [`migrate`] moves such a computer over.
//!
//! A frozen computer (UWF / Shadow Defender) is not moved: the installer updates it in place
//! (as before) and the move happens once the freeze is off — a move under a freeze would be
//! thrown away at the next restart and leave the computer without a working Vimora.
//!
//! Ubuntu: `/opt/vimora` (package), received files in `/opt/vimora/files`.

use std::path::{Path, PathBuf};

/// The program file's usual name: up to 0.3.27 `vimora-client.exe` (still used in place on a
/// frozen computer that was not moved yet), now `vimora-student.exe`.
pub fn exe_name() -> &'static str {
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    if me.starts_with("vimora-client") {
        if cfg!(windows) {
            "vimora-client.exe"
        } else {
            "vimora-client"
        }
    } else if cfg!(windows) {
        "vimora-student.exe"
    } else {
        "vimora-student"
    }
}

/// The folder of the running program.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn here() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// `C:\Program Files (x86)\Vimora`.
#[cfg(windows)]
pub fn canonical_dir() -> PathBuf {
    let pf = std::env::var_os("ProgramFiles(x86)")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .unwrap_or_else(|| r"C:\Program Files (x86)".into());
    PathBuf::from(pf).join("Vimora")
}

/// `C:\Program Files\Vimora` (0.3.27 and older), when it is a different folder.
#[cfg(windows)]
pub fn legacy_dir() -> Option<PathBuf> {
    let pf = std::env::var_os("ProgramW6432").or_else(|| std::env::var_os("ProgramFiles"))?;
    let d = PathBuf::from(pf).join("Vimora");
    (!same(&d, &canonical_dir())).then_some(d)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .to_lowercase()
        .trim_end_matches(['\\', '/'])
        == b.to_string_lossy()
            .to_lowercase()
            .trim_end_matches(['\\', '/'])
}

/// Files the teacher sends: inside Vimora's own folder (no folder in Downloads any more).
pub fn files_dir() -> PathBuf {
    #[cfg(windows)]
    let own = here().map(|d| d.join("Files"));
    #[cfg(not(windows))]
    let own = Some(PathBuf::from("/opt/vimora/files"));
    if let Some(d) = own {
        if writable(&d) {
            return d;
        }
    }
    // Not set up (an older install, a developer build): the user's Downloads as before.
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Vimora")
}

fn writable(d: &Path) -> bool {
    if std::fs::create_dir_all(d).is_err() {
        return false;
    }
    let probe = d.join(format!(".w{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Service start (Windows): moves a computer still in the 0.3.27 folder into the new one, and
/// clears the old folder once the new one runs. Nothing happens while the computer is frozen.
#[cfg(windows)]
pub fn migrate(protected: bool) {
    if protected {
        return;
    }
    let Some(me_dir) = here() else {
        return;
    };
    let canon = canonical_dir();
    let Some(legacy) = legacy_dir() else {
        return;
    };
    if same(&me_dir, &canon) {
        clear_legacy(&legacy);
        return;
    }
    if !same(&me_dir, &legacy) {
        return; // a developer build somewhere else: leave it alone
    }
    tracing::info!("moving Vimora to {}", canon.display());
    if std::fs::create_dir_all(&canon).is_err() {
        return;
    }
    let target = canon.join("vimora-student.exe");
    let Ok(me) = std::env::current_exe() else {
        return;
    };
    if std::fs::copy(&me, &target).is_err() {
        tracing::warn!("move: cannot copy the program to {}", target.display());
        return;
    }
    for f in ["uninstall.exe", "vimora.ico"] {
        let _ = std::fs::copy(legacy.join(f), canon.join(f));
    }
    // The new copy registers itself everywhere (service, tasks, Run key, firewall, uninstall
    // entry) and starts the service from the new folder; this one steps aside.
    let ok = std::process::Command::new(&target)
        .arg("setup")
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        tracing::info!("moved: the service continues from {}", canon.display());
        std::process::exit(0);
    }
    tracing::warn!("move: setup in the new folder failed; staying here");
}

#[cfg(windows)]
fn clear_legacy(legacy: &Path) {
    if !legacy.exists() {
        return;
    }
    // A teacher program installed there by an older version stays.
    if legacy.join("vimora-server.exe").exists() || legacy.join("vimora-teacher.exe").exists() {
        for f in ["vimora-client.exe", "uninstall.exe"] {
            let _ = std::fs::remove_file(legacy.join(f));
        }
        return;
    }
    // Old copies still running from there (an agent started before the move).
    let _ = crate::sh::run("taskkill", &["/F", "/IM", "vimora-client.exe"]);
    std::thread::sleep(std::time::Duration::from_secs(1));
    if std::fs::remove_dir_all(legacy).is_ok() {
        tracing::info!("old folder {} removed", legacy.display());
    }
}

#[cfg(not(windows))]
pub fn migrate(_protected: bool) {}

/// The language chosen in the setup window ("uz", "ru", "en"; Uzbek when none).
pub fn language() -> &'static str {
    #[cfg(windows)]
    let v = crate::sh::run("reg", &["query", r"HKLM\SOFTWARE\Vimora", "/v", "Language"])
        .map(|(_, out)| out)
        .unwrap_or_default();
    #[cfg(not(windows))]
    let v = std::fs::read_to_string("/etc/vimora/language").unwrap_or_default();
    let v = v.to_lowercase();
    if v.contains(" ru") || v.trim() == "ru" {
        "ru"
    } else if v.contains(" en") || v.trim() == "en" {
        "en"
    } else {
        "uz"
    }
}
