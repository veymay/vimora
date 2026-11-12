//! Notification-area ("tray") icon with a small menu, for the teacher and the student agent.
//!
//! * Windows: `tray-icon` on its own thread with its own message loop.
//! * Linux: StatusNotifierItem over D-Bus (`ksni`, pure Rust) — shown by Ubuntu's AppIndicator
//!   support, KDE, XFCE and others. Without a tray host nothing is shown and nothing breaks.
//!
//! The icon lives as long as the returned [`Tray`] (dropping it removes the icon).

use std::sync::Arc;

/// Straight RGBA pixels.
#[derive(Clone)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Clone)]
pub struct Item {
    pub id: &'static str,
    pub label: String,
    pub enabled: bool,
}

impl Item {
    pub fn new(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
        }
    }

    /// A greyed-out line (status text).
    pub fn text(label: impl Into<String>) -> Self {
        Self {
            id: "",
            label: label.into(),
            enabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The icon itself was clicked.
    Open,
    /// A menu item was chosen (its id).
    Menu(&'static str),
    /// A tray without menu items: the icon was clicked (left or right) at this screen position
    /// (pixels).
    Click(i32, i32),
}

pub struct Spec {
    /// Stable id ("vimora-teacher", "vimora-agent").
    pub id: &'static str,
    pub title: String,
    pub tooltip: String,
    pub icon: Rgba,
    pub items: Vec<Item>,
}

type Callback = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Tray(imp::Handle);

impl Tray {
    /// Shows the icon. `None` if this desktop has no tray (logged, not an error).
    pub fn spawn(spec: Spec, on_event: impl Fn(Event) + Send + Sync + 'static) -> Option<Tray> {
        imp::spawn(spec, Arc::new(on_event)).map(Tray)
    }

    pub fn set_tooltip(&self, text: String) {
        self.0.set_tooltip(text);
    }

    pub fn set_items(&self, items: Vec<Item>) {
        self.0.set_items(items);
    }
}

// ---------------------------------------------------------------------------------------------

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;
    use ksni::blocking::TrayMethods;

    struct Sni {
        spec: Spec,
        cb: Callback,
    }

    impl ksni::Tray for Sni {
        fn id(&self) -> String {
            self.spec.id.into()
        }
        fn title(&self) -> String {
            self.spec.title.clone()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            let i = &self.spec.icon;
            // RGBA → ARGB (network byte order).
            let mut data = Vec::with_capacity(i.data.len());
            for px in i.data.as_chunks::<4>().0 {
                data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
            }
            vec![ksni::Icon {
                width: i.width as i32,
                height: i.height as i32,
                data,
            }]
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.spec.title.clone(),
                description: self.spec.tooltip.clone(),
                ..Default::default()
            }
        }
        fn activate(&mut self, x: i32, y: i32) {
            if self.spec.items.is_empty() {
                (self.cb)(Event::Click(x, y));
            } else {
                (self.cb)(Event::Open);
            }
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            self.spec
                .items
                .iter()
                .map(|it| {
                    let id = it.id;
                    ksni::menu::StandardItem {
                        label: it.label.clone(),
                        enabled: it.enabled,
                        activate: Box::new(move |t: &mut Self| (t.cb)(Event::Menu(id))),
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        }
    }

    pub struct Handle(ksni::blocking::Handle<Sni>);

    impl Handle {
        pub fn set_tooltip(&self, text: String) {
            self.0.update(|t| t.spec.tooltip = text);
        }
        pub fn set_items(&self, items: Vec<Item>) {
            self.0.update(|t| t.spec.items = items);
        }
    }

    pub fn spawn(spec: Spec, cb: Callback) -> Option<Handle> {
        // At login the panel may come up after us: keep waiting for it instead of giving up.
        let sni = Sni { spec, cb };
        match sni.assume_sni_available(true).spawn() {
            Ok(h) => Some(Handle(h)),
            Err(e) => {
                tracing::info!("no tray on this desktop: {e}");
                None
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::mpsc;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, TranslateMessage, MSG,
        PM_REMOVE, QS_ALLINPUT,
    };

    enum Update {
        Tooltip(String),
        Items(Vec<Item>),
    }

    pub struct Handle(mpsc::Sender<Update>);

    impl Handle {
        pub fn set_tooltip(&self, text: String) {
            let _ = self.0.send(Update::Tooltip(text));
        }
        pub fn set_items(&self, items: Vec<Item>) {
            let _ = self.0.send(Update::Items(items));
        }
    }

    fn menu(items: &[Item]) -> Menu {
        let m = Menu::new();
        for it in items {
            let _ = m.append(&MenuItem::with_id(it.id, &it.label, it.enabled, None));
        }
        m
    }

    pub fn spawn(spec: Spec, cb: Callback) -> Option<Handle> {
        let (tx, rx) = mpsc::channel::<Update>();
        std::thread::Builder::new()
            .name("tray".into())
            .spawn(move || {
                let icon = match tray_icon::Icon::from_rgba(
                    spec.icon.data.clone(),
                    spec.icon.width,
                    spec.icon.height,
                ) {
                    Ok(i) => i,
                    Err(e) => return tracing::warn!("tray icon: {e}"),
                };
                let mut items = spec.items.clone();
                // No items: no menu — every click (left or right) is reported with its position.
                let clicks_only = items.is_empty();
                let mut builder = TrayIconBuilder::new()
                    .with_icon(icon)
                    .with_tooltip(&spec.tooltip);
                if !clicks_only {
                    builder = builder.with_menu(Box::new(menu(&items)));
                }
                let tray = match builder.build() {
                    Ok(t) => t,
                    Err(e) => return tracing::warn!("tray: {e}"),
                };
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                loop {
                    unsafe {
                        MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 100, QS_ALLINPUT);
                        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                    while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                        if let TrayIconEvent::Click {
                            button,
                            button_state: MouseButtonState::Up,
                            position,
                            ..
                        } = ev
                        {
                            if clicks_only {
                                if matches!(button, MouseButton::Left | MouseButton::Right) {
                                    cb(Event::Click(position.x as i32, position.y as i32));
                                }
                            } else if button == MouseButton::Left {
                                cb(Event::Open);
                            }
                        }
                    }
                    while let Ok(ev) = MenuEvent::receiver().try_recv() {
                        if let Some(it) = items.iter().find(|i| !i.id.is_empty() && ev.id.0 == i.id)
                        {
                            cb(Event::Menu(it.id));
                        }
                    }
                    loop {
                        match rx.try_recv() {
                            Ok(Update::Tooltip(t)) => {
                                let _ = tray.set_tooltip(Some(&t));
                            }
                            Ok(Update::Items(new)) => {
                                if !clicks_only {
                                    tray.set_menu(Some(Box::new(menu(&new))));
                                    items = new;
                                }
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            // Owner gone: remove the icon.
                            Err(mpsc::TryRecvError::Disconnected) => return,
                        }
                    }
                }
            })
            .ok()?;
        Some(Handle(tx))
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
mod imp {
    use super::*;
    pub struct Handle;
    impl Handle {
        pub fn set_tooltip(&self, _: String) {}
        pub fn set_items(&self, _: Vec<Item>) {}
    }
    pub fn spawn(_: Spec, _: Callback) -> Option<Handle> {
        None
    }
}

/// Decodes a PNG-free raw icon: helper for callers that already have RGBA bytes.
pub fn rgba(width: u32, height: u32, data: Vec<u8>) -> Rgba {
    Rgba {
        width,
        height,
        data,
    }
}
