//! The code that opens the teacher program: asked at start and again after the chosen minutes
//! without a touch. A full-window screen; the keyboard or the keypad types the digits.

use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Rounding, Sense, Stroke};
use std::time::{Duration, Instant};
use vimora_ui::{self as ui_kit, tr, Palette};

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
    /// When the screen appeared (fade-in), when the last digit came, when the right code was
    /// entered (fade-out), and how many digits were there a moment ago.
    shown_at: Instant,
    typed_at: Option<Instant>,
    unlocking: Option<Instant>,
    prev_len: usize,
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
            shown_at: Instant::now(),
            typed_at: None,
            unlocking: None,
            prev_len: 0,
        }
    }

    /// Lock now (the lock icon in the status bar).
    pub fn lock_now(&mut self) {
        self.locked = true;
        self.entered.clear();
        self.shown_at = Instant::now();
        self.unlocking = None;
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

    /// Draws the lock screen over the whole window and checks the code: a calm gradient with a
    /// soft glow, the logo, and the code cells; it fades in, the cells pop as digits come, shake
    /// on a wrong code, and the whole screen fades out when the code is right.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, c: &crate::config::TeacherConfig) {
        if !self.locked {
            return;
        }
        let n = (c.app_pin_len as usize).clamp(4, 8);
        // Digits are not taken while the screen is leaving.
        if self.unlocking.is_some() {
            self.entered.clear();
        }
        if self.entered.len() != self.prev_len {
            if self.entered.len() > self.prev_len {
                self.typed_at = Some(Instant::now());
            }
            self.prev_len = self.entered.len();
        }
        let ease = |t: f32| 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
        let fade_in = ease(self.shown_at.elapsed().as_secs_f32() / 0.45);
        let leave = self
            .unlocking
            .map(|t| (t.elapsed().as_secs_f32() / 0.35).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let fade = fade_in * (1.0 - leave * leave);
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("applock"))
            .order(egui::Order::Debug)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                // Eats every click so nothing below can be touched.
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
                let painter = ui.painter();
                // Background: a vertical gradient (the theme's two surfaces).
                let (top, bottom) = (p.panel, p.bg);
                let mut mesh = egui::epaint::Mesh::default();
                for (pt, col) in [
                    (rect.left_top(), top),
                    (rect.right_top(), top),
                    (rect.right_bottom(), bottom),
                    (rect.left_bottom(), bottom),
                ] {
                    mesh.colored_vertex(pt, col.gamma_multiply(fade.max(0.001)));
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(egui::Shape::mesh(mesh));

                let cx = rect.center().x;
                let top_y = rect.center().y - 95.0;
                let centre = pos2(cx, top_y);
                // A steady glow behind the logo.
                for (radius, alpha) in [(360.0, 0.10), (220.0, 0.16), (120.0, 0.20)] {
                    glow(painter, centre, radius, p.accent, alpha * fade);
                }
                // The logo grows into place as it fades in.
                let size = 64.0 * (0.88 + 0.12 * fade_in);
                if fade > 0.02 {
                    let lr = Rect::from_center_size(centre, vec2(size, size));
                    ui_kit::logo(painter, lr);
                    if fade < 1.0 {
                        // The logo has no alpha of its own: dim it with the background.
                        painter.rect_filled(lr, Rounding::ZERO, p.bg.gamma_multiply(1.0 - fade));
                    }
                }
                painter.text(
                    pos2(cx, top_y + 58.0),
                    Align2::CENTER_CENTER,
                    tr("Kodni kiriting"),
                    FontId::new(16.0, ui_kit::bold()),
                    p.muted.gamma_multiply(fade),
                );

                // Code cells: pop when a digit comes, shake after a wrong code, green at the end.
                let wrong = self
                    .wrong_at
                    .map(|t| t.elapsed().as_secs_f32())
                    .filter(|a| *a < 0.5);
                let shake = wrong
                    .map(|a| (a * 46.0).sin() * 14.0 * (1.0 - a / 0.5))
                    .unwrap_or(0.0);
                let pop = self
                    .typed_at
                    .map(|t| t.elapsed().as_secs_f32())
                    .filter(|a| *a < 0.18)
                    .map(|a| (a / 0.18 * std::f32::consts::PI).sin())
                    .unwrap_or(0.0);
                let (cell, gap) = (46.0, 12.0);
                let total = n as f32 * cell + (n as f32 - 1.0) * gap;
                let x0 = cx - total / 2.0 + shake;
                let row_y = top_y + 98.0;
                let ok = self.unlocking.is_some();
                for i in 0..n {
                    let on = i < self.entered.len() || ok;
                    let grow = if i + 1 == self.entered.len() && !ok {
                        1.0 + 0.14 * pop
                    } else {
                        1.0
                    };
                    let r = Rect::from_center_size(
                        pos2(
                            x0 + i as f32 * (cell + gap) + cell / 2.0,
                            row_y + cell / 2.0,
                        ),
                        vec2(cell, cell) * grow,
                    );
                    let edge = if ok {
                        p.success
                    } else if wrong.is_some() {
                        p.danger
                    } else if on || i == self.entered.len() {
                        p.accent
                    } else {
                        p.line_strong
                    };
                    painter.rect(
                        r,
                        Rounding::ZERO,
                        if on {
                            p.sunken
                        } else {
                            p.card.gamma_multiply(0.6)
                        }
                        .gamma_multiply(fade),
                        Stroke::new(
                            if i == self.entered.len() && !ok {
                                2.0_f32
                            } else {
                                1.0_f32
                            },
                            edge.gamma_multiply(fade),
                        ),
                    );
                    if on {
                        painter.circle_filled(
                            r.center(),
                            5.5 * grow,
                            if ok { p.success } else { p.strong }.gamma_multiply(fade),
                        );
                    }
                }
                // After many wrong codes: how long to wait.
                if let Some(d) = self
                    .blocked_until
                    .and_then(|t| t.checked_duration_since(Instant::now()))
                {
                    painter.text(
                        pos2(cx, row_y + cell + 26.0),
                        Align2::CENTER_CENTER,
                        format!("{} s", d.as_secs() + 1),
                        FontId::new(13.0, ui_kit::mono()),
                        p.danger.gamma_multiply(fade),
                    );
                }
            });
        if self.unlocking.is_some() {
            if leave >= 1.0 {
                self.locked = false;
                self.unlocking = None;
                self.fails = 0;
                self.last_active = Instant::now();
            }
        } else if self.entered.len() >= n {
            if check_pin(c, &self.entered) {
                self.unlocking = Some(Instant::now());
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
                self.entered.clear();
            }
        }
        ctx.request_repaint();
    }
}

/// A soft round glow (colour fading to nothing at the edge).
fn glow(
    painter: &egui::Painter,
    centre: egui::Pos2,
    radius: f32,
    color: egui::Color32,
    alpha: f32,
) {
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(centre, color.gamma_multiply(alpha));
    const N: usize = 72;
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            centre + vec2(a.cos(), a.sin()) * radius,
            egui::Color32::TRANSPARENT,
        );
    }
    for i in 0..N {
        mesh.add_triangle(0, 1 + i as u32, 1 + ((i + 1) % N) as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Dev aid: `VIMORA_PIN=1234` runs the program with that code (for screenshots).
pub(super) fn dev_pin(c: &mut crate::config::TeacherConfig) {
    if let Ok(pin) = std::env::var("VIMORA_PIN") {
        set_pin(c, &pin);
    }
}
