//! Keyboard / mouse lock without the lock screen (Windows): low-level hooks swallow the
//! student's own input; injected input (the teacher's remote control) still passes.

use anyhow::Result;

pub fn set(keyboard: bool, mouse: bool) -> Result<String> {
    imp::set(keyboard, mouse)?;
    Ok(match (keyboard, mouse) {
        (false, false) => "klaviatura va sichqoncha ochildi".into(),
        (true, true) => "klaviatura va sichqoncha bloklandi".into(),
        (true, false) => "klaviatura bloklandi".into(),
        (false, true) => "sichqoncha bloklandi".into(),
    })
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static KB: AtomicBool = AtomicBool::new(false);
    static MS: AtomicBool = AtomicBool::new(false);
    /// Thread running the hooks (0 = none).
    static THREAD: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn kb_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 && KB.load(Ordering::Relaxed) {
            let k = &*(l as *const KBDLLHOOKSTRUCT);
            if k.flags & LLKHF_INJECTED == 0 {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }

    unsafe extern "system" fn ms_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 && MS.load(Ordering::Relaxed) {
            let m = &*(l as *const MSLLHOOKSTRUCT);
            if m.flags & LLMHF_INJECTED == 0 {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }

    pub fn set(keyboard: bool, mouse: bool) -> anyhow::Result<()> {
        KB.store(keyboard, Ordering::Relaxed);
        MS.store(mouse, Ordering::Relaxed);
        let running = THREAD.load(Ordering::Relaxed);
        if (keyboard || mouse) && running == 0 {
            std::thread::Builder::new()
                .name("input-lock".into())
                .spawn(|| unsafe {
                    THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);
                    let module = GetModuleHandleW(std::ptr::null());
                    let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), module, 0);
                    let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(ms_proc), module, 0);
                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    UnhookWindowsHookEx(kb);
                    UnhookWindowsHookEx(ms);
                    THREAD.store(0, Ordering::Relaxed);
                })?;
        } else if !keyboard && !mouse && running != 0 {
            unsafe {
                PostThreadMessageW(running, WM_QUIT, 0, 0);
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn set(keyboard: bool, mouse: bool) -> anyhow::Result<()> {
        if keyboard || mouse {
            anyhow::bail!("klaviatura/sichqonchani alohida bloklash hozircha faqat Windows'da");
        }
        Ok(())
    }
}
