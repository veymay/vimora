//! Remote-control input injection.
//!
//! * Windows: `SendInput` (unicode text + virtual keys).
//! * Linux/BSD: XTEST extension through pure-Rust x11rb.

use crate::protocol::InputEvent;
use anyhow::Result;

pub struct Injector {
    inner: imp::Injector,
}

impl Injector {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: imp::Injector::new()?,
        })
    }

    pub fn inject(&mut self, ev: &InputEvent) -> Result<()> {
        self.inner.inject(ev)
    }

    /// Sends Ctrl+W to the focused window (closes the current browser tab).
    pub fn close_tab(&mut self) -> Result<()> {
        self.inner.close_tab()
    }
}

// ---------------------------------------------------------------------------------------------
#[cfg(windows)]
mod imp {
    use crate::protocol::{InputEvent, MouseButton, SpecialKey};
    use anyhow::Result;
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    pub struct Injector;

    fn send(inputs: &[INPUT]) {
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            );
        }
    }

    fn mouse(dx: i32, dy: i32, data: i32, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: data as _,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key(vk: u16, scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn vk(k: SpecialKey) -> u16 {
        match k {
            SpecialKey::Enter => VK_RETURN,
            SpecialKey::Backspace => VK_BACK,
            SpecialKey::Tab => VK_TAB,
            SpecialKey::Escape => VK_ESCAPE,
            SpecialKey::Delete => VK_DELETE,
            SpecialKey::Left => VK_LEFT,
            SpecialKey::Right => VK_RIGHT,
            SpecialKey::Up => VK_UP,
            SpecialKey::Down => VK_DOWN,
            SpecialKey::Home => VK_HOME,
            SpecialKey::End => VK_END,
            SpecialKey::PageUp => VK_PRIOR,
            SpecialKey::PageDown => VK_NEXT,
            SpecialKey::Space => VK_SPACE,
        }
    }

    impl Injector {
        pub fn new() -> Result<Self> {
            Ok(Self)
        }

        pub fn close_tab(&mut self) -> Result<()> {
            send(&[
                key(VK_CONTROL, 0, 0),
                key(0x57, 0, 0),
                key(0x57, 0, KEYEVENTF_KEYUP),
                key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
            ]);
            Ok(())
        }

        pub fn inject(&mut self, ev: &InputEvent) -> Result<()> {
            match ev {
                InputEvent::MouseMove { x, y } => {
                    let dx = (x.clamp(0.0, 1.0) * 65535.0) as i32;
                    let dy = (y.clamp(0.0, 1.0) * 65535.0) as i32;
                    send(&[mouse(
                        dx,
                        dy,
                        0,
                        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    )]);
                }
                InputEvent::MouseButton { button, down } => {
                    let f = match (button, down) {
                        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                    };
                    send(&[mouse(0, 0, 0, f)]);
                }
                InputEvent::Scroll { dx, dy } => {
                    if *dy != 0.0 {
                        send(&[mouse(0, 0, (*dy * 120.0) as i32, MOUSEEVENTF_WHEEL)]);
                    }
                    if *dx != 0.0 {
                        send(&[mouse(0, 0, (*dx * 120.0) as i32, MOUSEEVENTF_HWHEEL)]);
                    }
                }
                InputEvent::Text(t) => {
                    let mut v = Vec::new();
                    for u in t.encode_utf16() {
                        v.push(key(0, u, KEYEVENTF_UNICODE));
                        v.push(key(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                    }
                    send(&v);
                }
                InputEvent::Key { key: k, down } => {
                    let flags = if *down { 0 } else { KEYEVENTF_KEYUP };
                    send(&[key(vk(*k), 0, flags)]);
                }
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------------------------
#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use crate::protocol::{InputEvent, MouseButton, SpecialKey};
    use anyhow::{Context, Result};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        ConnectionExt as _, Window, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT,
        KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
    };
    use x11rb::protocol::xtest::ConnectionExt as _;
    use x11rb::rust_connection::RustConnection;

    const XK_SHIFT_L: u32 = 0xffe1;

    pub struct Injector {
        conn: RustConnection,
        root: Window,
        width: u16,
        height: u16,
        min_keycode: u8,
        per: usize,
        keysyms: Vec<u32>,
    }

    fn special_keysym(k: SpecialKey) -> u32 {
        match k {
            SpecialKey::Enter => 0xff0d,
            SpecialKey::Backspace => 0xff08,
            SpecialKey::Tab => 0xff09,
            SpecialKey::Escape => 0xff1b,
            SpecialKey::Delete => 0xffff,
            SpecialKey::Left => 0xff51,
            SpecialKey::Up => 0xff52,
            SpecialKey::Right => 0xff53,
            SpecialKey::Down => 0xff54,
            SpecialKey::Home => 0xff50,
            SpecialKey::End => 0xff57,
            SpecialKey::PageUp => 0xff55,
            SpecialKey::PageDown => 0xff56,
            SpecialKey::Space => 0x20,
        }
    }

    impl Injector {
        pub fn new() -> Result<Self> {
            let (conn, n) = x11rb::connect(None).context("cannot connect to X11 display")?;
            let setup = conn.setup();
            let screen = &setup.roots[n];
            let (root, width, height) =
                (screen.root, screen.width_in_pixels, screen.height_in_pixels);
            let (min, max) = (setup.min_keycode, setup.max_keycode);
            let map = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
            Ok(Self {
                root,
                width,
                height,
                min_keycode: min,
                per: map.keysyms_per_keycode as usize,
                keysyms: map.keysyms,
                conn,
            })
        }

        /// Returns (keycode, needs_shift).
        fn lookup(&self, keysym: u32) -> Option<(u8, bool)> {
            let i = self.keysyms.iter().position(|&k| k == keysym)?;
            let per = self.per.max(1);
            Some((self.min_keycode + (i / per) as u8, i % per == 1))
        }

        fn fake(&self, ty: u8, detail: u8, x: i16, y: i16) -> Result<()> {
            self.conn
                .xtest_fake_input(ty, detail, 0, self.root, x, y, 0)?;
            Ok(())
        }

        fn tap_keysym(&self, keysym: u32) -> Result<()> {
            if let Some((code, shift)) = self.lookup(keysym) {
                let shift_code = if shift {
                    self.lookup(XK_SHIFT_L).map(|s| s.0)
                } else {
                    None
                };
                if let Some(s) = shift_code {
                    self.fake(KEY_PRESS_EVENT, s, 0, 0)?;
                }
                self.fake(KEY_PRESS_EVENT, code, 0, 0)?;
                self.fake(KEY_RELEASE_EVENT, code, 0, 0)?;
                if let Some(s) = shift_code {
                    self.fake(KEY_RELEASE_EVENT, s, 0, 0)?;
                }
            }
            Ok(())
        }

        pub fn close_tab(&mut self) -> Result<()> {
            if let (Some((ctrl, _)), Some((w, _))) = (self.lookup(0xffe3), self.lookup(0x77)) {
                self.fake(KEY_PRESS_EVENT, ctrl, 0, 0)?;
                self.fake(KEY_PRESS_EVENT, w, 0, 0)?;
                self.fake(KEY_RELEASE_EVENT, w, 0, 0)?;
                self.fake(KEY_RELEASE_EVENT, ctrl, 0, 0)?;
                self.conn.flush()?;
            }
            Ok(())
        }

        pub fn inject(&mut self, ev: &InputEvent) -> Result<()> {
            match ev {
                InputEvent::MouseMove { x, y } => {
                    let px = (x.clamp(0.0, 1.0) * (self.width as f32 - 1.0)) as i16;
                    let py = (y.clamp(0.0, 1.0) * (self.height as f32 - 1.0)) as i16;
                    self.fake(MOTION_NOTIFY_EVENT, 0, px, py)?;
                }
                InputEvent::MouseButton { button, down } => {
                    let b = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                    };
                    self.fake(
                        if *down {
                            BUTTON_PRESS_EVENT
                        } else {
                            BUTTON_RELEASE_EVENT
                        },
                        b,
                        0,
                        0,
                    )?;
                }
                InputEvent::Scroll { dx, dy } => {
                    let click = |b: u8, n: f32| -> Result<()> {
                        for _ in 0..(n.abs().round().max(1.0) as u32) {
                            self.fake(BUTTON_PRESS_EVENT, b, 0, 0)?;
                            self.fake(BUTTON_RELEASE_EVENT, b, 0, 0)?;
                        }
                        Ok(())
                    };
                    if *dy != 0.0 {
                        click(if *dy > 0.0 { 4 } else { 5 }, *dy)?;
                    }
                    if *dx != 0.0 {
                        click(if *dx > 0.0 { 6 } else { 7 }, *dx)?;
                    }
                }
                InputEvent::Text(t) => {
                    for c in t.chars() {
                        let ks = if (c as u32) < 0x100 {
                            c as u32
                        } else {
                            0x0100_0000 | c as u32
                        };
                        self.tap_keysym(ks)?;
                    }
                }
                InputEvent::Key { key, down } => {
                    if let Some((code, _)) = self.lookup(special_keysym(*key)) {
                        self.fake(
                            if *down {
                                KEY_PRESS_EVENT
                            } else {
                                KEY_RELEASE_EVENT
                            },
                            code,
                            0,
                            0,
                        )?;
                    }
                }
            }
            self.conn.flush()?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod imp {
    use crate::protocol::InputEvent;
    use anyhow::{bail, Result};
    pub struct Injector;
    impl Injector {
        pub fn new() -> Result<Self> {
            bail!("remote control is not implemented on macOS yet")
        }
        pub fn inject(&mut self, _ev: &InputEvent) -> Result<()> {
            Ok(())
        }
        pub fn close_tab(&mut self) -> Result<()> {
            Ok(())
        }
    }
}
