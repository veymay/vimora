//! Live view of one computer, shown full-size in the main area below the navbar, with
//! remote control.

use super::grid::status_badge;
use super::*;

/// Height of the viewer's toolbar.
const BAR_H: f32 = 52.0;

pub(super) fn special_key(k: Key) -> Option<SpecialKey> {
    Some(match k {
        Key::Enter => SpecialKey::Enter,
        Key::Backspace => SpecialKey::Backspace,
        Key::Tab => SpecialKey::Tab,
        Key::Escape => SpecialKey::Escape,
        Key::Delete => SpecialKey::Delete,
        Key::ArrowLeft => SpecialKey::Left,
        Key::ArrowRight => SpecialKey::Right,
        Key::ArrowUp => SpecialKey::Up,
        Key::ArrowDown => SpecialKey::Down,
        Key::Home => SpecialKey::Home,
        Key::End => SpecialKey::End,
        Key::PageUp => SpecialKey::PageUp,
        Key::PageDown => SpecialKey::PageDown,
        _ => return None,
    })
}

/// What the toolbar asked for this frame.
#[derive(Default)]
struct ViewerActions {
    back: bool,
    toggle_control: bool,
    lock: Option<Command>,
    message: bool,
    goto: Option<u64>,
}

impl ConsoleApp {
    /// Replaces the computer list while a computer is being watched.
    pub(super) fn viewer_page(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let Some(v) = &self.viewer else { return };
        let id = v.id;
        let control = v.control;
        let Some(info) = all.iter().find(|s| s.id == id) else {
            self.viewer = None; // computer disconnected
            return;
        };
        let tex = self.textures.get(&id).map(|t| t.1.clone());
        let (prev, next) = neighbours(&self.visible_order, id);
        let mut act = ViewerActions::default();
        let mut inputs: Vec<InputEvent> = Vec::new();
        let mut last_move = self.viewer.as_ref().and_then(|v| v.last_move);

        egui::CentralPanel::default()
            .frame(Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                // --- toolbar ------------------------------------------------------------------
                let (bar, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), BAR_H), Sense::hover());
                ui.painter().rect_filled(bar, Rounding::ZERO, p.panel);
                ui.painter().hline(
                    bar.x_range(),
                    bar.bottom() - 0.5,
                    Stroke::new(1.0_f32, p.line),
                );
                ui.allocate_new_ui(
                    egui::UiBuilder::new()
                        .max_rect(bar)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if ui_kit::icon_cell(ui, icons::ARROW_LEFT, BAR_H, false)
                            .on_hover_text("Ro'yxatga qaytish")
                            .clicked()
                        {
                            act.back = true;
                        }
                        ui_kit::rule(ui, BAR_H);
                        if ui_kit::icon_cell(ui, icons::CHEVRON_LEFT, BAR_H, false)
                            .on_hover_text("Oldingi kompyuter")
                            .clicked()
                        {
                            act.goto = prev;
                        }
                        if ui_kit::icon_cell(ui, icons::CHEVRON_RIGHT, BAR_H, false)
                            .on_hover_text("Keyingi kompyuter")
                            .clicked()
                        {
                            act.goto = next;
                        }
                        ui_kit::rule(ui, BAR_H);
                        ui.add_space(16.0);
                        ui_kit::tag(ui, &format!("{:02}", info.number), p.hover, p.text);
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(&info.label)
                                .family(ui_kit::bold())
                                .size(16.0)
                                .color(p.strong),
                        );
                        ui.add_space(14.0);
                        let res = if info.frame_size.0 > 0 {
                            format!(" · {}×{}", info.frame_size.0, info.frame_size.1)
                        } else {
                            String::new()
                        };
                        ui_kit::hud(ui, &format!("{} · {}{res}", info.user, info.ip), p.muted);

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.add_space(8.0);
                            let (kind, label) = if control {
                                (Kind::Danger, "Boshqaruvni to'xtatish")
                            } else {
                                (Kind::Primary, "Boshqarish")
                            };
                            if ui_kit::button(ui, Some(icons::MOUSE_POINTER_2), label, kind)
                                .on_hover_text(
                                    "Sichqoncha va klaviaturani shu kompyuterga yuborish",
                                )
                                .clicked()
                            {
                                act.toggle_control = true;
                            }
                            if info.status.locked {
                                if ui_kit::button(
                                    ui,
                                    Some(icons::LOCK_OPEN),
                                    "Ochish",
                                    Kind::Outline,
                                )
                                .clicked()
                                {
                                    act.lock = Some(Command::Unlock);
                                }
                            } else if ui_kit::button(
                                ui,
                                Some(icons::LOCK),
                                "Qulflash",
                                Kind::Outline,
                            )
                            .clicked()
                            {
                                let c = self.cfg.read().unwrap();
                                act.lock = Some(Command::Lock {
                                    title: c.lock_title.clone(),
                                    message: c.lock_message.clone(),
                                });
                            }
                            if ui_kit::button(
                                ui,
                                Some(icons::MESSAGE_SQUARE_TEXT),
                                "Xabar",
                                Kind::Outline,
                            )
                            .clicked()
                            {
                                act.message = true;
                            }
                            ui.add_space(8.0);
                            let badge = status_badge(ui, p, &info.status);
                            let (r, _) =
                                ui.allocate_exact_size(vec2(badge.width, 20.0), Sense::hover());
                            badge.paint(ui.painter(), r);
                        });
                    },
                );

                // --- screen ---------------------------------------------------------------------
                let (area, resp) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                let Some(tex) = &tex else {
                    ui.painter().text(
                        area.center() - vec2(0.0, 14.0),
                        Align2::CENTER_CENTER,
                        icons::MONITOR,
                        FontId::proportional(36.0),
                        p.faint,
                    );
                    let g = ui_kit::caps(ui, "Ekran kutilmoqda", 10.0, ui_kit::mono(), p.faint);
                    ui.painter().galley(
                        pos2(area.center().x - g.size().x / 2.0, area.center().y + 18.0),
                        g,
                        p.faint,
                    );
                    return;
                };
                let s = tex.size_vec2();
                let scale = (area.width() / s.x).min(area.height() / s.y);
                let img = Rect::from_center_size(area.center(), s * scale);
                ui.painter().image(
                    tex.id(),
                    img,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );

                if !control {
                    return;
                }
                // Remote control: frame the screen and forward input.
                ui.painter().rect_stroke(
                    img.expand(1.0),
                    Rounding::ZERO,
                    Stroke::new(2.0_f32, p.danger),
                );
                let hint = ui_kit::caps(
                    ui,
                    "Boshqaruv faol · klaviatura shu kompyuterga yuboriladi",
                    9.5,
                    ui_kit::mono(),
                    Color32::WHITE,
                );
                let hr =
                    Rect::from_min_size(pos2(img.left(), img.top()), hint.size() + vec2(16.0, 8.0));
                ui.painter().rect_filled(hr, Rounding::ZERO, p.danger);
                ui.painter()
                    .galley(hr.min + vec2(8.0, 4.0), hint, Color32::WHITE);
                if resp.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                let norm = |pos: Pos2| {
                    (
                        (pos.x - img.left()) / img.width(),
                        (pos.y - img.top()) / img.height(),
                    )
                };
                for ev in ui.input(|i| i.events.clone()) {
                    match ev {
                        Event::PointerMoved(pos) if img.contains(pos) => {
                            let n = norm(pos);
                            if last_move != Some(n) {
                                last_move = Some(n);
                                inputs.push(InputEvent::MouseMove { x: n.0, y: n.1 });
                            }
                        }
                        Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            ..
                        } if img.contains(pos) => {
                            let b = match button {
                                egui::PointerButton::Primary => MouseButton::Left,
                                egui::PointerButton::Secondary => MouseButton::Right,
                                egui::PointerButton::Middle => MouseButton::Middle,
                                _ => continue,
                            };
                            let n = norm(pos);
                            inputs.push(InputEvent::MouseMove { x: n.0, y: n.1 });
                            inputs.push(InputEvent::MouseButton {
                                button: b,
                                down: pressed,
                            });
                        }
                        Event::MouseWheel { delta, .. } if resp.hovered() => {
                            inputs.push(InputEvent::Scroll {
                                dx: delta.x.signum(),
                                dy: delta.y.signum(),
                            });
                        }
                        Event::Text(t) => inputs.push(InputEvent::Text(t)),
                        Event::Key { key, pressed, .. } => {
                            if let Some(k) = special_key(key) {
                                inputs.push(InputEvent::Key {
                                    key: k,
                                    down: pressed,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });

        if let Some(v) = &mut self.viewer {
            v.last_move = last_move;
            if act.toggle_control {
                v.control = !v.control;
            }
        }
        if !inputs.is_empty() {
            let h = self.hub.lock().unwrap();
            for ev in inputs {
                h.send(&[id], &ServerMsg::Input(ev));
            }
        }
        if let Some(c) = act.lock {
            self.send(&[id], c, "");
        }
        if act.message {
            self.selected = [id].into();
            self.open_dialog(Dialog::Message {
                text: String::new(),
                send: false,
            });
        }
        if let Some(other) = act.goto {
            self.open_viewer(other, false);
        } else if act.back {
            self.close_viewer();
        }
    }
}

/// Previous / next computer in the on-screen order (wrapping).
fn neighbours(order: &[u64], id: u64) -> (Option<u64>, Option<u64>) {
    let Some(i) = order.iter().position(|x| *x == id) else {
        return (None, None);
    };
    if order.len() < 2 {
        return (None, None);
    }
    let prev = order[(i + order.len() - 1) % order.len()];
    let next = order[(i + 1) % order.len()];
    (Some(prev), Some(next))
}

#[cfg(test)]
mod tests {
    use super::neighbours;

    #[test]
    fn wraps_around() {
        assert_eq!(neighbours(&[1, 2, 3], 1), (Some(3), Some(2)));
        assert_eq!(neighbours(&[1, 2, 3], 3), (Some(2), Some(1)));
        assert_eq!(neighbours(&[1], 1), (None, None));
        assert_eq!(neighbours(&[1, 2], 9), (None, None));
    }
}
