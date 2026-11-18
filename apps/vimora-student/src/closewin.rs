//! "Close every program": a polite close (WM_CLOSE / `_NET_CLOSE_WINDOW`) of all the signed-in
//! user's open program windows. VIMORA's own windows, the desktop and the taskbar stay.

use anyhow::Result;

#[cfg(windows)]
pub fn close_all() -> Result<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowTextLengthW,
        GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GWL_EXSTYLE, GW_OWNER, WM_CLOSE,
        WS_EX_TOOLWINDOW,
    };

    fn image_of(hwnd: HWND) -> String {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return String::new();
            }
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0;
            CloseHandle(h);
            if !ok {
                return String::new();
            }
            String::from_utf16_lossy(&buf[..n as usize])
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or("")
                .to_lowercase()
        }
    }

    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> i32 {
        let closed = &mut *(lp as *mut usize);
        if IsWindowVisible(hwnd) == 0
            || !GetWindow(hwnd, GW_OWNER).is_null()
            || GetWindowTextLengthW(hwnd) == 0
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
        {
            return 1;
        }
        let mut class = [0u16; 128];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return 1;
        }
        let image = image_of(hwnd);
        // VIMORA's own windows (lock screen, chat, banner…) and the shell stay.
        if image.starts_with("vimora") || image == "explorer.exe" && class != "CabinetWClass" {
            return 1;
        }
        if PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 {
            *closed += 1;
        }
        1
    }

    let mut closed = 0usize;
    unsafe {
        EnumWindows(Some(each), &mut closed as *mut usize as LPARAM);
    }
    Ok(format!("{closed} ta oyna yopildi"))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn close_all() -> Result<String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask};
    let (conn, screen) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen].root;
    let atom =
        |name: &str| -> Result<u32> { Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom) };
    let list = atom("_NET_CLIENT_LIST")?;
    let close = atom("_NET_CLOSE_WINDOW")?;
    let name = atom("_NET_WM_NAME")?;
    let reply = conn
        .get_property(false, root, list, AtomEnum::WINDOW, 0, 4096)?
        .reply()?;
    let windows: Vec<u32> = reply.value32().map(|v| v.collect()).unwrap_or_default();
    let mut closed = 0;
    for w in windows {
        // VIMORA's own windows stay (their titles begin with the name).
        let title = conn
            .get_property(false, w, name, AtomEnum::ANY, 0, 64)?
            .reply()
            .map(|r| String::from_utf8_lossy(&r.value).to_string())
            .unwrap_or_default();
        if title.starts_with("VIMORA") {
            continue;
        }
        let ev = ClientMessageEvent::new(32, w, close, [0, 2, 0, 0, 0]);
        conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        closed += 1;
    }
    conn.flush()?;
    Ok(format!("{closed} ta oyna yopildi"))
}

#[cfg(target_os = "macos")]
pub fn close_all() -> Result<String> {
    anyhow::bail!("bu tizimda ishlamaydi")
}
