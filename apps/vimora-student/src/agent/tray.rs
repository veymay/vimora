//! The student program's tray icon: shows that Vimora is running. A click (left or right) opens
//! the small Vimora window in the middle of the screen (`vimora-student about`): version, the teacher computer,
//! updates. No "quit": the program must keep running.

use super::*;
use vimora_tray::{Event, Spec, Tray};

/// The open tray window (one at a time).
static WINDOW: Mutex<Option<std::process::Child>> = Mutex::new(None);

fn open_window(state: &Shared) {
    let mut w = WINDOW.lock().unwrap();
    // Still open: a second click closes it (like a menu), so it never piles up.
    if let Some(child) = w.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
            *w = None;
            return;
        }
    }
    let (teacher, connected) = {
        let s = state.lock().unwrap();
        (s.teacher.clone().unwrap_or_default(), s.connected)
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("about")
        .env("VIMORA_TEACHER", teacher)
        .env("VIMORA_CONNECTED", if connected { "1" } else { "0" });
    match cmd.spawn() {
        Ok(child) => *w = Some(child),
        Err(e) => warn!("tray window: {e}"),
    }
}

pub(super) fn start(state: Shared) {
    let _ = std::thread::Builder::new()
        .name("student-tray".into())
        .spawn(move || {
            let png = include_bytes!("../../../../assets/brand/vimora-64.png");
            let Some(img) = image::load_from_memory(png).ok().map(|i| i.to_rgba8()) else {
                return;
            };
            let icon = vimora_tray::rgba(img.width(), img.height(), img.into_raw());
            let version = format!("Vimora {}", env!("CARGO_PKG_VERSION"));
            let st = state.clone();
            let Some(tray) = Tray::spawn(
                Spec {
                    id: "vimora-student",
                    title: "Vimora".into(),
                    tooltip: version.clone(),
                    icon,
                    items: vec![],
                },
                move |ev| {
                    if let Event::Click(..) = ev {
                        open_window(&st);
                    }
                },
            ) else {
                return;
            };
            let mut last = None;
            loop {
                let now = {
                    let s = state.lock().unwrap();
                    (s.connected, s.teacher.clone())
                };
                if last.as_ref() != Some(&now) {
                    let line = match &now {
                        (true, Some(t)) => format!("{version}\n{t}"),
                        _ => version.clone(),
                    };
                    tray.set_tooltip(line);
                    last = Some(now);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
}
