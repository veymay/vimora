//! Windows: the lock screen (and the teacher's presentation) run on a desktop of their own.
//!
//! On that desktop there is no taskbar, no Start menu and no other window, so Alt+Tab, the
//! Windows key, Win+D & co. have nothing to switch to — the screen simply can't change. It is the
//! same mechanism Windows uses for the UAC prompt. Ctrl+Alt+Del is still handled by Windows
//! (see `kiosk`).
//!
//! Safety: whoever ends the overlay brings the normal desktop back ([`restore`]): the agent after
//! it stops or loses the overlay, the overlay itself if it panics, and the agent once at start-up.
//! If the lock desktop can't be created or shown, the overlay stays on the normal desktop.

#[cfg(windows)]
mod imp {
    use std::ptr::null;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::GENERIC_ALL;
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, CreateDesktopW, GetThreadDesktop, OpenDesktopW, SetThreadDesktop,
        SwitchDesktop, DESKTOP_SWITCHDESKTOP, HDESK,
    };
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;

    const NAME: &str = "VimoraLock";
    /// The lock desktop of this process (0 = none).
    static LOCK: AtomicIsize = AtomicIsize::new(0);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Overlay process, first thing on the main thread (before any window or hook): create the
    /// lock desktop, move this thread onto it and show it. `false` = stay on the normal desktop.
    pub fn enter() -> bool {
        unsafe {
            let original: HDESK = GetThreadDesktop(GetCurrentThreadId());
            let name = wide(NAME);
            let desk = CreateDesktopW(name.as_ptr(), null(), null(), 0, GENERIC_ALL, null());
            if desk.is_null() {
                tracing::warn!("lock desktop: CreateDesktop failed");
                return false;
            }
            if SetThreadDesktop(desk) == 0 {
                tracing::warn!("lock desktop: SetThreadDesktop failed");
                CloseDesktop(desk);
                return false;
            }
            if SwitchDesktop(desk) == 0 {
                tracing::warn!("lock desktop: SwitchDesktop failed");
                SetThreadDesktop(original);
                CloseDesktop(desk);
                return false;
            }
            LOCK.store(desk as isize, Ordering::Relaxed);
            // Whatever goes wrong in here, the student must get their normal screen back.
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore();
                previous(info);
            }));
            true
        }
    }

    /// Other threads of the overlay (keyboard hooks, the guard) join the lock desktop, so their
    /// hooks and window lookups apply there. No-op when there is none.
    pub fn join_thread() {
        let d = LOCK.load(Ordering::Relaxed);
        if d != 0 {
            unsafe {
                SetThreadDesktop(d as HDESK);
            }
        }
    }

    /// Show the normal desktop ("Default") again.
    pub fn restore() {
        unsafe {
            let name = wide("Default");
            let def = OpenDesktopW(name.as_ptr(), 0, 0, DESKTOP_SWITCHDESKTOP);
            if !def.is_null() {
                SwitchDesktop(def);
                CloseDesktop(def);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn enter() -> bool {
        false
    }
    pub fn restore() {}
}

#[cfg(windows)]
pub use imp::join_thread;
pub use imp::{enter, restore};
