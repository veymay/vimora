//! The student agent's tray icon: shows that Vimora is running and whether a console is
//! connected. No "quit": the agent must keep running.

use super::*;
use vimora_tray::{Item, Spec, Tray};

pub(super) fn start(state: Shared) {
    let _ = std::thread::Builder::new()
        .name("agent-tray".into())
        .spawn(move || {
            let png = include_bytes!("../../../../assets/brand/vimora-64.png");
            let Some(img) = image::load_from_memory(png).ok().map(|i| i.to_rgba8()) else {
                return;
            };
            let icon = vimora_tray::rgba(img.width(), img.height(), img.into_raw());
            let version = format!("Vimora {}", env!("CARGO_PKG_VERSION"));
            let Some(tray) = Tray::spawn(
                Spec {
                    id: "vimora-agent",
                    title: "Vimora".into(),
                    tooltip: version.clone(),
                    icon,
                    items: vec![Item::text(version.clone())],
                },
                |_| {},
            ) else {
                return;
            };
            let mut last = None;
            loop {
                let now = {
                    let s = state.lock().unwrap();
                    (s.connected, s.lock.is_some())
                };
                if last != Some(now) {
                    last = Some(now);
                    let line = match now {
                        (_, true) => "Ekran qulflangan",
                        (true, false) => "O'qituvchi konsoliga ulangan",
                        (false, false) => "O'qituvchi konsoli kutilmoqda",
                    };
                    tray.set_tooltip(format!("{version}\n{line}"));
                    tray.set_items(vec![Item::text(line), Item::text(version.clone())]);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
}
