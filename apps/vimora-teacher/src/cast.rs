//! The wireless displays drawer's engine: finds Google Cast displays and shows the teacher's
//! screen on one (see `vimora-cast`). One cast at a time; it lives in this process, so closing
//! the window (the program stays in the tray) does not end it.

use std::sync::{Arc, Mutex, OnceLock};
use vimora_cast::{Device, Discovery, FrameSource, Session, State};
use vimora_core::capture;

pub struct Manager {
    rt: tokio::runtime::Runtime,
    discovery: Mutex<Option<Discovery>>,
    session: Mutex<Option<Session>>,
    /// When the current search began (the drawer shows "searching" for the first seconds).
    scan_started: Mutex<std::time::Instant>,
}

pub fn manager() -> &'static Manager {
    static M: OnceLock<Manager> = OnceLock::new();
    M.get_or_init(|| Manager {
        rt: tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("cast runtime"),
        discovery: Mutex::new(None),
        session: Mutex::new(None),
        scan_started: Mutex::new(std::time::Instant::now()),
    })
}

impl Manager {
    /// Starts looking for displays (once; the list keeps itself current).
    pub fn ensure_discovery(&self) {
        let mut d = self.discovery.lock().unwrap();
        if d.is_none() {
            *d = Some(Discovery::start());
            *self.scan_started.lock().unwrap() = std::time::Instant::now();
        }
    }

    /// Looks again from scratch (a display switched on a moment ago).
    pub fn rescan(&self) {
        let mut d = self.discovery.lock().unwrap();
        *d = None;
        *d = Some(Discovery::start());
        *self.scan_started.lock().unwrap() = std::time::Instant::now();
    }

    /// Seconds since the search began.
    pub fn scan_age(&self) -> f32 {
        self.scan_started.lock().unwrap().elapsed().as_secs_f32()
    }

    pub fn devices(&self) -> Vec<Device> {
        self.discovery
            .lock()
            .unwrap()
            .as_ref()
            .map(|d| d.devices())
            .unwrap_or_default()
    }

    /// The display being cast to (or tried), and how it is going.
    pub fn current(&self) -> Option<(Device, State)> {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| (s.device().clone(), s.state()))
    }

    /// Casts the screen to `device` (ending any other cast first).
    pub fn connect(&self, device: Device) {
        let mut s = self.session.lock().unwrap();
        if let Some(old) = s.take() {
            old.stop();
        }
        *s = Some(Session::start(self.rt.handle(), device, screen_source()));
    }

    pub fn stop(&self) {
        if let Some(s) = self.session.lock().unwrap().take() {
            s.stop();
        }
    }
}

/// Where the pictures come from: the screen (X11 / Windows) or, on Wayland, the portal stream.
enum Screen {
    Grab(Box<capture::Capturer>),
    #[cfg(all(unix, not(target_os = "macos")))]
    Portal(crate::wayland_cast::Cast, Vec<u8>),
}

impl Screen {
    fn open() -> Option<Screen> {
        #[cfg(all(unix, not(target_os = "macos")))]
        if crate::wayland_cast::is_wayland() {
            return crate::wayland_cast::Cast::start()
                .map_err(|e| tracing::warn!("cast: screen share refused: {e:#}"))
                .ok()
                .map(|c| Screen::Portal(c, Vec::new()));
        }
        let mut c = capture::Capturer::new().ok()?;
        c.set_cursor(true);
        Some(Screen::Grab(Box::new(c)))
    }

    /// The screen now as a JPEG; `Err` = the source is gone.
    fn jpeg(&mut self) -> Result<Option<Vec<u8>>, ()> {
        match self {
            Screen::Grab(c) => c
                .capture()
                .and_then(|f| capture::encode_jpeg(&f, 1920, 75))
                .map(|(_, _, j)| Some(j))
                .map_err(|_| ()),
            #[cfg(all(unix, not(target_os = "macos")))]
            Screen::Portal(c, last) => match c.latest() {
                Ok(Some(f)) => {
                    *last = f.jpeg.clone();
                    Ok(Some(f.jpeg))
                }
                // A still screen gives no new frames: keep showing the last one.
                Ok(None) => Ok((!last.is_empty()).then(|| last.clone())),
                Err(_) => Err(()),
            },
        }
    }
}

fn screen_source() -> FrameSource {
    let screen: Mutex<Option<Screen>> = Mutex::new(None);
    Arc::new(move || {
        let mut g = screen.lock().unwrap();
        if g.is_none() {
            *g = Screen::open();
        }
        match g.as_mut()?.jpeg() {
            Ok(j) => j,
            Err(()) => {
                *g = None; // opened again on the next try
                None
            }
        }
    })
}
