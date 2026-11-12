//! Explorer-style selection of computers.
//!
//! * click → select only that computer;
//! * Ctrl+click → add / remove it;
//! * Shift+click → range from the last clicked one (Ctrl+Shift adds the range);
//! * drag on the list → rubber-band rectangle; Ctrl/Shift while dragging adds to the selection;
//! * click on empty space → clear;
//! * the list scrolls by itself while the rectangle touches its top or bottom edge.

use super::*;

/// An in-progress rubber-band drag.
pub(super) struct Marquee {
    /// Where the drag started, relative to the top-left of the scrolled content (so the
    /// rectangle stays anchored while the list scrolls).
    origin: Vec2,
    /// Selection when the drag started (kept when adding with Ctrl/Shift).
    base: BTreeSet<u64>,
}

/// Item rectangles laid out this frame, in screen coordinates.
pub(super) type Hitboxes = Vec<(u64, Rect)>;

const AUTOSCROLL_EDGE: f32 = 36.0;
const AUTOSCROLL_SPEED: f32 = 14.0;

impl TeacherApp {
    /// Selection change for a click on an item.
    pub(super) fn click_select(&mut self, id: u64, mods: egui::Modifiers) {
        if mods.shift {
            if let Some(anchor) = self.anchor {
                let order = &self.visible_order;
                if let (Some(a), Some(b)) = (
                    order.iter().position(|x| *x == anchor),
                    order.iter().position(|x| *x == id),
                ) {
                    let (lo, hi) = (a.min(b), a.max(b));
                    if !mods.command {
                        self.selected.clear();
                    }
                    self.selected.extend(order[lo..=hi].iter().copied());
                    return;
                }
            }
        }
        if mods.command {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
        } else {
            self.selected = [id].into();
        }
        self.anchor = Some(id);
    }

    /// Background interaction of the list: rubber band, click-to-clear and auto-scroll.
    /// Call after laying out the items; `bg` must be registered *before* the items so that
    /// clicks on items win while drags anywhere start a rectangle.
    pub(super) fn marquee(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        bg: &egui::Response,
        content_origin: Pos2,
        items: &Hitboxes,
    ) {
        let mods = ui.input(|i| i.modifiers);
        let adding = mods.command || mods.shift;

        if bg.drag_started_by(egui::PointerButton::Primary) {
            let start = ui
                .input(|i| i.pointer.press_origin())
                .unwrap_or(content_origin);
            self.marquee = Some(Marquee {
                origin: start - content_origin,
                base: if adding {
                    self.selected.clone()
                } else {
                    BTreeSet::new()
                },
            });
        }

        if let Some(m) = &self.marquee {
            let pointer = ui.input(|i| i.pointer.interact_pos());
            let held = ui.input(|i| i.pointer.primary_down());
            if let (Some(cur), true) = (pointer, held) {
                let band = Rect::from_two_pos(content_origin + m.origin, cur);
                let mut sel = m.base.clone();
                sel.extend(
                    items
                        .iter()
                        .filter(|(_, r)| r.intersects(band))
                        .map(|(id, _)| *id),
                );
                self.selected = sel;

                let clip = ui.clip_rect();
                let painter = ui.painter_at(clip);
                painter.rect(
                    band.intersect(clip),
                    Rounding::ZERO,
                    p.accent.gamma_multiply(0.12),
                    Stroke::new(1.0_f32, p.accent),
                );

                if cur.y > clip.bottom() - AUTOSCROLL_EDGE {
                    ui.scroll_with_delta(vec2(0.0, -AUTOSCROLL_SPEED));
                } else if cur.y < clip.top() + AUTOSCROLL_EDGE {
                    ui.scroll_with_delta(vec2(0.0, AUTOSCROLL_SPEED));
                }
                ui.ctx().request_repaint();
            } else {
                self.marquee = None;
            }
        } else if bg.clicked() && !adding {
            self.selected.clear();
            self.anchor = None;
        }
    }
}
