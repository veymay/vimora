//! OS integration: power, launching, file drops, app and website blocking.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use vimora_core::PowerAction;

pub fn set_dpi_aware() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "?".into())
}

pub fn hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".into())
}

pub fn open_url(url: &str) -> Result<()> {
    let url = url.trim();
    let url = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("only http(s) links are allowed");
    }
    #[cfg(windows)]
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()?;
    #[cfg(target_os = "macos")]
    Command::new("open").arg(&url).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(&url).spawn()?;
    Ok(())
}

pub fn run_program(program: &str, args: &[String]) -> Result<()> {
    Command::new(program)
        .args(args)
        .spawn()
        .with_context(|| format!("cannot start {program}"))?;
    Ok(())
}

pub fn power(action: PowerAction) -> Result<()> {
    #[cfg(windows)]
    let (prog, args): (&str, Vec<&str>) = match action {
        PowerAction::Shutdown => ("shutdown", vec!["/s", "/t", "0", "/f"]),
        PowerAction::Restart => ("shutdown", vec!["/r", "/t", "0", "/f"]),
        PowerAction::Logoff => ("shutdown", vec!["/l", "/f"]),
    };
    #[cfg(not(windows))]
    let user = username();
    #[cfg(not(windows))]
    let (prog, args): (&str, Vec<&str>) = match action {
        PowerAction::Shutdown => ("systemctl", vec!["poweroff"]),
        PowerAction::Restart => ("systemctl", vec!["reboot"]),
        PowerAction::Logoff => ("loginctl", vec!["terminate-user", user.as_str()]),
    };
    Command::new(prog)
        .args(&args)
        .spawn()
        .with_context(|| format!("{prog} failed"))?;
    Ok(())
}

/// Hardware identity: MAC of the first physical network card (lower-case, no separators).
pub fn machine_id() -> String {
    #[cfg(windows)]
    {
        // getmac /fo csv /nh → "AA-BB-CC-DD-EE-FF","\Device\Tcpip_{…}"
        let out = crate::sh::run("getmac", &["/fo", "csv", "/nh"])
            .map(|(_, o)| o)
            .unwrap_or_default();
        out.lines()
            .filter_map(|l| l.split(',').next())
            .map(|m| m.trim_matches('"').replace('-', "").to_lowercase())
            .find(|m| m.len() == 12 && m != "000000000000")
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let mut ifaces: Vec<_> = std::fs::read_dir("/sys/class/net")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        ifaces.sort();
        ifaces
            .iter()
            .filter(|p| p.join("device").exists()) // physical NICs only
            .filter_map(|p| std::fs::read_to_string(p.join("address")).ok())
            .map(|m| m.trim().replace(':', "").to_lowercase())
            .find(|m| m.len() == 12 && m != "000000000000")
            .unwrap_or_default()
    }
}

/// Changes the system computer name (Windows: effective after a restart).
pub fn rename_computer(name: &str) -> Result<()> {
    if !vimora_core::valid_computer_name(name) {
        bail!("noto'g'ri nom: 1–15 ta harf, raqam yoki '-'");
    }
    #[cfg(windows)]
    crate::sh::run_ok(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("Rename-Computer -NewName '{name}' -Force -ErrorAction Stop"),
        ],
    )?;
    #[cfg(not(windows))]
    {
        crate::sh::run_ok("hostnamectl", &["set-hostname", name])?;
        let hosts = std::fs::read_to_string("/etc/hosts").unwrap_or_default();
        let fixed: String = hosts
            .lines()
            .map(|l| {
                if l.starts_with("127.0.1.1") {
                    format!("127.0.1.1\t{name}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write("/etc/hosts", fixed + "\n")?;
    }
    Ok(())
}

/// Files a rename changes (Linux; committed in Shadow mode).
pub fn rename_touched() -> Vec<std::path::PathBuf> {
    if cfg!(windows) {
        vec![]
    } else {
        vec!["/etc/hostname".into(), "/etc/hosts".into()]
    }
}

/// Saves a file sent by the teacher into `Downloads/Vimora/`.
pub fn save_file(name: &str, data: &[u8]) -> Result<PathBuf> {
    let safe = std::path::Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty() && s != "." && s != "..")
        .unwrap_or_else(|| "file.bin".into());
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Vimora");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(safe);
    std::fs::write(&path, data)?;
    Ok(path)
}

// ---------------------------------------------------------------------------------------------
// App blocking

fn norm_proc(name: &str) -> String {
    let n = name.trim().to_lowercase();
    n.strip_suffix(".exe").map(str::to_string).unwrap_or(n)
}

pub struct AppBlocker {
    sys: sysinfo::System,
}

impl AppBlocker {
    pub fn new() -> Self {
        Self {
            sys: sysinfo::System::new(),
        }
    }

    /// Terminates every running process whose name is on the list. Returns killed names.
    pub fn enforce(&mut self, list: &[String]) -> Vec<String> {
        if list.is_empty() {
            return vec![];
        }
        let wanted: Vec<String> = list
            .iter()
            .map(|s| norm_proc(s))
            .filter(|s| !s.is_empty())
            .collect();
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let me = std::process::id();
        let mut killed = vec![];
        for (pid, p) in self.sys.processes() {
            if pid.as_u32() == me {
                continue;
            }
            let name = norm_proc(&p.name().to_string_lossy());
            if wanted.contains(&name) && p.kill() {
                killed.push(name);
            }
        }
        killed
    }
}

/// Shape of the mouse pointer right now ([`vimora_core::cursor_kind`]); `None` where the
/// platform can't tell.
pub fn cursor_kind() -> Option<u8> {
    #[cfg(windows)]
    {
        use vimora_core::cursor_kind as k;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            let mut ci: CURSORINFO = std::mem::zeroed();
            ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
            if GetCursorInfo(&mut ci) == 0 {
                return None;
            }
            let table = [
                (IDC_ARROW, k::ARROW),
                (IDC_IBEAM, k::TEXT),
                (IDC_HAND, k::HAND),
                (IDC_WAIT, k::WAIT),
                (IDC_SIZEWE, k::SIZE_WE),
                (IDC_SIZENS, k::SIZE_NS),
                (IDC_SIZENWSE, k::SIZE_NWSE),
                (IDC_SIZENESW, k::SIZE_NESW),
                (IDC_SIZEALL, k::MOVE),
                (IDC_CROSS, k::CROSS),
                (IDC_NO, k::NO),
                (IDC_APPSTARTING, k::PROGRESS),
            ];
            for (id, kind) in table {
                if LoadCursorW(std::ptr::null_mut(), id) == ci.hCursor {
                    return Some(kind);
                }
            }
            Some(k::ARROW)
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}
