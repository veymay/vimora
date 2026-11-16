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
    {
        // Starting something is not opening a page: look for the browser window afterwards.
        let before = browser_windows::snapshot();
        open_target(&url)?;
        browser_windows::confirm(&before)
    }
    #[cfg(not(windows))]
    {
        open_target(&url)
    }
}

/// Windows: tells whether a browser window really showed up (or changed) after a launch.
#[cfg(windows)]
mod browser_windows {
    use anyhow::{bail, Result};
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    const BROWSERS: [&str; 11] = [
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "brave.exe",
        "opera.exe",
        "vivaldi.exe",
        "browser.exe",
        "chromium.exe",
        "iexplore.exe",
        "yandex.exe",
        "arc.exe",
    ];

    fn exe_of(hwnd: HWND) -> Option<String> {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0;
            CloseHandle(h);
            ok.then(|| {
                let full = String::from_utf16_lossy(&buf[..n as usize]);
                full.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()
            })
        }
    }

    fn title_of(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// Visible browser windows now: handle → title.
    pub fn snapshot() -> HashMap<isize, String> {
        unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> i32 {
            let map = &mut *(lp as *mut HashMap<isize, String>);
            if IsWindowVisible(hwnd) != 0 {
                if let Some(exe) = exe_of(hwnd) {
                    if BROWSERS.contains(&exe.as_str()) {
                        map.insert(hwnd as isize, title_of(hwnd));
                    }
                }
            }
            1
        }
        let mut map: HashMap<isize, String> = HashMap::new();
        unsafe {
            EnumWindows(Some(each), &mut map as *mut _ as LPARAM);
        }
        map
    }

    /// Waits a few seconds for a new browser window, a changed title (a new tab) or a browser
    /// that came to the front; says so honestly when nothing happened.
    pub fn confirm(before: &HashMap<isize, String>) -> Result<()> {
        let front_before = unsafe { GetForegroundWindow() } as isize;
        for _ in 0..24 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let now = snapshot();
            let changed = now
                .iter()
                .any(|(h, t)| before.get(h).is_none_or(|old| old != t));
            let front = unsafe { GetForegroundWindow() } as isize;
            if changed || (front != front_before && now.contains_key(&front)) {
                return Ok(());
            }
        }
        bail!("brauzer ishga tushirildi, lekin oyna paydo bo'lmadi (brauzer o'rnatilmagan yoki bloklangan bo'lishi mumkin)")
    }
}

/// Opens a file (or a link) the way a double click does: the default program for it. The
/// teacher's "open after sending" and "Ochish" use this for files under the Files folder.
pub fn open_path(path: &std::path::Path) -> Result<()> {
    open_target(&path.to_string_lossy())
}

/// Windows: the command line of the default browser for web pages, read from the registry
/// (what a double click on a link would run), with the address in place.
#[cfg(windows)]
fn default_browser_command(url: &str) -> Option<(String, Vec<String>)> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let query = |key: &str, value: Option<&str>| -> Option<String> {
        let mut c = Command::new("reg");
        c.arg("query").arg(key);
        match value {
            Some(v) => c.args(["/v", v]),
            None => c.arg("/ve"),
        };
        let out = c.creation_flags(NO_WINDOW).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        // "    ProgId    REG_SZ    ChromeHTML" / "    (Default)    REG_SZ    "C:\...\chrome.exe" -- "%1""
        text.lines().find_map(|l| {
            let l = l.trim();
            let i = l.find("REG_SZ").or_else(|| l.find("REG_EXPAND_SZ"))?;
            let rest = l[i..].split_once(char::is_whitespace)?.1.trim();
            (!rest.is_empty()).then(|| rest.to_string())
        })
    };
    let prog = query(
        r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
        Some("ProgId"),
    )?;
    let cmd = query(&format!(r"HKCR\{prog}\shell\open\command"), None)?;
    let cmd = cmd.replace("%1", url).replace("%L", url);
    // Split "<program> <args>": the program may be quoted.
    let (exe, rest) = if let Some(r) = cmd.strip_prefix('"') {
        let end = r.find('"')?;
        (r[..end].to_string(), r[end + 1..].trim().to_string())
    } else {
        let (a, b) = cmd.split_once(' ').unwrap_or((cmd.as_str(), ""));
        (a.to_string(), b.to_string())
    };
    // The arguments as typed (they may be quoted); a plain split on spaces outside quotes.
    let mut args = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    for ch in rest.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    if !rest.contains(url) {
        args.push(url.to_string());
    }
    Some((exe, args))
}

fn open_target(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        // A web address: start the default browser straight from its registered command (the
        // shell's own "open" can silently do nothing for a protected background program).
        if url.starts_with("http://") || url.starts_with("https://") {
            if let Some((exe, args)) = default_browser_command(url) {
                match Command::new(&exe).args(&args).spawn() {
                    Ok(_) => {
                        tracing::info!("opened {url} with {exe}");
                        return Ok(());
                    }
                    Err(e) => tracing::warn!("default browser {exe}: {e}"),
                }
            }
        }
        // The system's own "open" (what a double click does) first, then helpers: the first
        // that starts wins.
        use std::os::windows::process::CommandExt;
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                keybd_event, KEYEVENTF_KEYUP, VK_MENU,
            };
            use windows_sys::Win32::UI::Shell::ShellExecuteW;
            use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
            // A background program may not bring a window to the front: the browser would
            // open behind the others (or only blink in the taskbar). A tap of Alt and the
            // foreground permission let the new page come up in front.
            unsafe {
                AllowSetForegroundWindow(ASFW_ANY);
                keybd_event(VK_MENU as u8, 0, 0, 0);
                keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
            }
            // ShellExecute needs COM on this thread (the page handlers use it); harmless when it
            // is already set up.
            unsafe {
                windows_sys::Win32::System::Com::CoInitializeEx(
                    std::ptr::null(),
                    windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32,
                );
            }
            let verb: Vec<u16> = "open\0".encode_utf16().collect();
            let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
            // SW_SHOWNORMAL = 1; a result above 32 means it worked.
            let r = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    target.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                )
            };
            if r as usize > 32 {
                return Ok(());
            }
            tracing::warn!("ShellExecute failed ({}); trying helpers", r as usize);
        }
        const NO_WINDOW: u32 = 0x0800_0000;
        let tries: [(&str, Vec<&str>); 3] = [
            ("rundll32", vec!["url.dll,FileProtocolHandler", url]),
            ("explorer.exe", vec![url]),
            ("cmd.exe", vec!["/c", "start", "", url]),
        ];
        let mut last = None;
        for (prog, args) in tries {
            match Command::new(prog)
                .args(&args)
                .creation_flags(NO_WINDOW)
                .spawn()
            {
                Ok(_) => return Ok(()),
                Err(e) => last = Some(e),
            }
        }
        if let Some(e) = last {
            bail!("brauzerni ochib bo'lmadi: {e}");
        }
    }
    #[cfg(target_os = "macos")]
    Command::new("open").arg(url).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(url).spawn()?;
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
/// The computer's identity for the teacher (its key): the MAC of a *physical* network card.
///
/// Disk images (AOMEI Backupper & co.) copy everything, so the identity must come from the
/// hardware, never from what was installed: virtual adapters (VirtualBox, VPN, Hyper-V) keep
/// their MAC in a clone. The chosen id is kept with a fingerprint of the physical cards; when the
/// disk turns up on other hardware (a clone), a new id is chosen. A computer that already has an
/// id keeps it (its name and room stay with the teacher).
pub fn machine_id() -> String {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(compute_machine_id).clone()
}

fn compute_machine_id() -> String {
    let physical = physical_macs();
    let fingerprint = physical.join(",");
    let file = dirs::config_dir().map(|d| d.join("vimora").join("machine-id"));
    let saved = file
        .as_ref()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| {
            let mut l = t.lines();
            (
                l.next().unwrap_or("").trim().to_string(),
                l.next().unwrap_or("").trim().to_string(),
            )
        });
    if let Some((id, hw)) = &saved {
        if !id.is_empty() && *hw == fingerprint {
            return id.clone();
        }
    }
    let legacy = legacy_mac();
    let id = match (&saved, physical.first()) {
        // No physical card found: the old way.
        (_, None) => legacy,
        // First run of this version on a computer: keep the id it had, if that is a real card.
        (None, Some(first)) => {
            if physical.contains(&legacy) {
                legacy
            } else {
                first.clone()
            }
        }
        // Saved on other hardware: this disk was cloned onto a new computer.
        (Some(_), Some(first)) => first.clone(),
    };
    if let Some(f) = file {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(f, format!("{id}\n{fingerprint}\n"));
    }
    id
}

/// MACs of the physical network cards (PCI / USB devices), sorted, lowercase hex.
fn physical_macs() -> Vec<String> {
    #[cfg(windows)]
    {
        let out = crate::sh::script(
            "Get-CimInstance Win32_NetworkAdapter -Filter 'PhysicalAdapter=TRUE' | \
             Where-Object { $_.MACAddress -and $_.PNPDeviceID -match '^(PCI|USB)\\\\' } | \
             ForEach-Object { $_.MACAddress }",
        )
        .map(|(_, o)| o)
        .unwrap_or_default();
        let mut v: Vec<String> = out
            .lines()
            .map(|m| m.trim().replace([':', '-'], "").to_lowercase())
            .filter(|m| m.len() == 12 && m != "000000000000")
            .collect();
        v.sort();
        v.dedup();
        v
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
        let mut v: Vec<String> = ifaces
            .iter()
            .filter(|p| p.join("device").exists()) // physical NICs only
            .filter_map(|p| std::fs::read_to_string(p.join("address")).ok())
            .map(|m| m.trim().replace(':', "").to_lowercase())
            .filter(|m| m.len() == 12 && m != "000000000000")
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// The id of 0.3.28 and older: the first MAC `getmac` lists (Windows), the first physical card
/// (Ubuntu).
fn legacy_mac() -> String {
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
            .filter(|p| p.join("device").exists())
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
/// Where files from the teacher land: Downloads/Vimora.
fn received_dir() -> PathBuf {
    crate::layout::files_dir()
}

/// "Papka/ichki/fayl.txt" → a path under `dir` that can't climb out of it.
fn safe_join(dir: &std::path::Path, name: &str) -> PathBuf {
    let mut p = dir.to_path_buf();
    for part in name.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            continue;
        }
        p.push(part);
    }
    if p == dir {
        p.push("file.bin");
    }
    p
}

/// Saves a file from the teacher (a folder's files keep their sub folders).
pub fn save_file(name: &str, data: &[u8]) -> Result<PathBuf> {
    let dir = received_dir();
    let path = safe_join(&dir, name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, data)?;
    Ok(path)
}

/// Opens a file the teacher sent, with its usual program.
pub fn open_received(name: &str) -> Result<String> {
    let path = safe_join(&received_dir(), name);
    if !path.exists() {
        anyhow::bail!("fayl topilmadi: {name}");
    }
    open_path(&path)?;
    Ok(format!("ochildi: {}", path.display()))
}

/// The student's assignment folder: Documents/Topshiriq (created when missing).
pub fn assignment_dir() -> PathBuf {
    let d = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Topshiriq");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Everything in the assignment folder (up to 50 MB in all).
pub fn collect_assignments() -> Vec<(String, Vec<u8>)> {
    const LIMIT: u64 = 50 * 1024 * 1024;
    let root = assignment_dir();
    let mut out = vec![];
    let mut total = 0u64;
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if total + size > LIMIT {
                continue;
            }
            if let Ok(data) = std::fs::read(&p) {
                total += size;
                let rel = p
                    .strip_prefix(&root)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                out.push((rel, data));
            }
        }
    }
    out
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

    /// "Faqat ruxsat etilgan dasturlar" (Windows): ends this user's programs that are not on
    /// the list. Windows' own components (anything under %WINDIR%), Vimora and a few shell
    /// helpers are never touched. Returns killed names.
    pub fn enforce_allow(&mut self, list: &[String]) -> Vec<String> {
        if list.is_empty() || !cfg!(windows) {
            return vec![];
        }
        const ALWAYS: &[&str] = &[
            "explorer",
            "vimora-student",
            "vimora-teacher",
            "searchhost",
            "startmenuexperiencehost",
            "shellexperiencehost",
            "textinputhost",
            "ctfmon",
            "runtimebroker",
            "applicationframehost",
            "sihost",
            "taskhostw",
            "dllhost",
            "conhost",
            "securityhealthsystray",
            "msedgewebview2",
            "widgets",
            "lockapp",
        ];
        let allowed: Vec<String> = list.iter().map(|s| norm_proc(s)).collect();
        let windir = std::env::var("WINDIR")
            .unwrap_or_else(|_| r"C:\Windows".into())
            .to_lowercase();
        self.sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::new()
                .with_user(sysinfo::UpdateKind::OnlyIfNotSet)
                .with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
        );
        let me = std::process::id();
        let my_user = self
            .sys
            .process(sysinfo::Pid::from_u32(me))
            .and_then(|p| p.user_id().cloned());
        let mut killed = vec![];
        for (pid, p) in self.sys.processes() {
            if pid.as_u32() == me || my_user.is_none() || p.user_id() != my_user.as_ref() {
                continue;
            }
            let Some(exe) = p.exe() else { continue };
            if exe.to_string_lossy().to_lowercase().starts_with(&windir) {
                continue;
            }
            let name = norm_proc(&p.name().to_string_lossy());
            if ALWAYS.contains(&name.as_str()) || allowed.contains(&name) {
                continue;
            }
            if p.kill() {
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
