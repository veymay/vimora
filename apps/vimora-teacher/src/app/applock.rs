//! The code that opens the teacher program: asked at start and again after the chosen minutes
//! without a touch. A full-window screen; the keyboard or the keypad types the digits.

use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Rounding, Sense, Stroke};
use std::time::{Duration, Instant};
use vimora_ui::{self as ui_kit, icons, tr, Palette};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Settings → "Kirish": sets (or, with an empty code, removes) the code.
pub(super) fn set_pin(c: &mut crate::config::TeacherConfig, pin: &str) {
    if pin.is_empty() {
        c.app_pin_salt.clear();
        c.app_pin_hash.clear();
        c.app_pin_len = 0;
        return;
    }
    let salt = vimora_core::auth::nonce();
    c.app_pin_hash = hex(&vimora_core::pin::hash(&salt, pin));
    c.app_pin_salt = hex(&salt);
    c.app_pin_len = pin.len() as u8;
}

fn check_pin(c: &crate::config::TeacherConfig, pin: &str) -> bool {
    let salt: Vec<u8> = (0..c.app_pin_salt.len() / 2)
        .filter_map(|i| u8::from_str_radix(&c.app_pin_salt[i * 2..i * 2 + 2], 16).ok())
        .collect();
    !c.app_pin_hash.is_empty() && hex(&vimora_core::pin::hash(&salt, pin)) == c.app_pin_hash
}

pub(super) struct AppLock {
    pub locked: bool,
    last_active: Instant,
    entered: String,
    wrong_at: Option<Instant>,
    fails: u32,
    blocked_until: Option<Instant>,
}

impl AppLock {
    pub fn new(has_pin: bool) -> Self {
        Self {
            locked: has_pin,
            last_active: Instant::now(),
            entered: String::new(),
            wrong_at: None,
            fails: 0,
            blocked_until: None,
        }
    }

    /// Lock now (the lock icon in the status bar).
    pub fn lock_now(&mut self) {
        self.locked = true;
        self.entered.clear();
    }

    /// Called first thing every frame: auto-lock after idle time, and while locked take the
    /// keyboard (so nothing below sees the digits). Returns true while the screen is locked.
    pub fn begin(&mut self, ctx: &egui::Context, c: &crate::config::TeacherConfig) -> bool {
        if c.app_pin_hash.is_empty() {
            self.locked = false;
            return false;
        }
        let touched =
            ctx.input(|i| i.pointer.is_moving() || i.pointer.any_down() || !i.events.is_empty());
        if touched && !self.locked {
            self.last_active = Instant::now();
        }
        if !self.locked && c.app_lock_min > 0 {
            let limit = Duration::from_secs(c.app_lock_min as u64 * 60);
            if self.last_active.elapsed() >= limit {
                self.lock_now();
            } else {
                ctx.request_repaint_after(Duration::from_secs(5));
            }
        }
        if self.locked {
            // The digits (and Backspace) are ours; nothing else gets any key.
            let events = ctx.input_mut(|i| std::mem::take(&mut i.events));
            let blocked = self.blocked_until.is_some_and(|t| Instant::now() < t);
            let mut keep = Vec::new();
            for e in events {
                match e {
                    egui::Event::Text(t) => {
                        if !blocked {
                            for ch in t.chars().filter(char::is_ascii_digit) {
                                self.entered.push(ch);
                            }
                        }
                    }
                    egui::Event::Key { key, pressed, .. } => {
                        if pressed && key == egui::Key::Backspace {
                            self.entered.pop();
                        }
                    }
                    // The pointer (the keypad needs it; the screen below cannot be reached
                    // anyway) and window events stay; keys, clipboard and the like are dropped.
                    egui::Event::Screenshot { .. }
                    | egui::Event::WindowFocused(_)
                    | egui::Event::MouseMoved(_)
                    | egui::Event::PointerMoved(_)
                    | egui::Event::PointerButton { .. }
                    | egui::Event::PointerGone => keep.push(e),
                    _ => {}
                }
            }
            ctx.input_mut(|i| i.events = keep);
        }
        self.locked
    }

    /// Draws the lock screen over the whole window and checks the code.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, c: &crate::config::TeacherConfig) {
        if !self.locked {
            return;
        }
        let n = (c.app_pin_len as usize).clamp(4, 8);
        let screen = ctx.screen_rect();
        let mut tapped: Option<char> = None;
        let mut back = false;
        egui::Area::new(egui::Id::new("applock"))
            .order(egui::Order::Debug)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                // Eats every click so nothing below can be touched.
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
                let painter = ui.painter();
                painter.rect_filled(rect, Rounding::ZERO, p.bg);
                let cx = rect.center().x;
                let mut y = (rect.center().y - 250.0).max(rect.top() + 24.0);
                ui_kit::logo(
                    painter,
                    Rect::from_center_size(pos2(cx, y + 24.0), vec2(48.0, 48.0)),
                );
                y += 72.0;
                painter.text(
                    pos2(cx, y + 10.0),
                    Align2::CENTER_CENTER,
                    tr("Kodni kiriting"),
                    FontId::new(17.0, ui_kit::bold()),
                    p.strong,
                );
                y += 40.0;
                // Code cells (shake after a wrong code).
                let wrong = self
                    .wrong_at
                    .map(|t| t.elapsed().as_secs_f32())
                    .filter(|a| *a < 0.5);
                let shake = wrong
                    .map(|a| (a * 46.0).sin() * 12.0 * (1.0 - a / 0.5))
                    .unwrap_or(0.0);
                let (cell, gap) = (40.0, 10.0);
                let total = n as f32 * cell + (n as f32 - 1.0) * gap;
                let x0 = cx - total / 2.0 + shake;
                for i in 0..n {
                    let r = Rect::from_min_size(
                        pos2(x0 + i as f32 * (cell + gap), y),
                        vec2(cell, cell),
                    );
                    let on = i < self.entered.len();
                    painter.rect(
                        r,
                        Rounding::ZERO,
                        p.sunken,
                        Stroke::new(
                            1.0_f32,
                            if wrong.is_some() {
                                p.danger
                            } else if on || i == self.entered.len() {
                                p.accent
                            } else {
                                p.line_strong
                            },
                        ),
                    );
                    if on {
                        painter.circle_filled(r.center(), 5.0, p.strong);
                    }
                }
                y += cell + 12.0;
                let blocked = self
                    .blocked_until
                    .and_then(|t| t.checked_duration_since(Instant::now()));
                painter.text(
                    pos2(cx, y + 8.0),
                    Align2::CENTER_CENTER,
                    match blocked {
                        Some(d) => format!("{} s", d.as_secs() + 1),
                        None => String::new(),
                    },
                    FontId::new(13.0, ui_kit::mono()),
                    p.danger,
                );
                y += 30.0;
                // The keypad: 1-9, backspace, 0.
                let key = vec2(68.0, 52.0);
                let kgap = 8.0;
                let kx = cx - (3.0 * key.x + 2.0 * kgap) / 2.0;
                let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "<", "0", ""];
                for (i, k) in keys.iter().enumerate() {
                    if k.is_empty() {
                        continue;
                    }
                    let r = Rect::from_min_size(
                        pos2(
                            kx + (i % 3) as f32 * (key.x + kgap),
                            y + (i / 3) as f32 * (key.y + kgap),
                        ),
                        key,
                    );
                    let resp = ui.interact(r, egui::Id::new(("applock-key", i)), Sense::click());
                    ui.painter().rect(
                        r,
                        Rounding::ZERO,
                        if resp.hovered() { p.hover } else { p.sunken },
                        Stroke::new(1.0_f32, p.line_strong),
                    );
                    if *k == "<" {
                        ui.painter().text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            icons::ARROW_LEFT,
                            FontId::proportional(18.0),
                            p.muted,
                        );
                    } else {
                        ui.painter().text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            *k,
                            FontId::new(20.0, ui_kit::mono_bold()),
                            p.strong,
                        );
                    }
                    if resp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        if *k == "<" {
                            back = true;
                        } else {
                            tapped = k.chars().next();
                        }
                    }
                }
            });
        if back {
            self.entered.pop();
        }
        let blocked = self.blocked_until.is_some_and(|t| Instant::now() < t);
        if let Some(d) = tapped.filter(|_| !blocked) {
            self.entered.push(d);
        }
        if self.entered.len() >= n {
            if check_pin(c, &self.entered) {
                self.locked = false;
                self.fails = 0;
                self.last_active = Instant::now();
            } else {
                self.wrong_at = Some(Instant::now());
                self.fails += 1;
                // After five wrong codes: a pause that grows.
                if self.fails.is_multiple_of(5) {
                    self.blocked_until = Some(
                        Instant::now()
                            + Duration::from_secs((30 * (self.fails as u64 / 5)).min(300)),
                    );
                }
            }
            self.entered.clear();
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// Dev aid: `VIMORA_PIN=1234` runs the program with that code (for screenshots).
pub(super) fn dev_pin(c: &mut crate::config::TeacherConfig) {
    if let Ok(pin) = std::env::var("VIMORA_PIN") {
        set_pin(c, &pin);
    }
}
