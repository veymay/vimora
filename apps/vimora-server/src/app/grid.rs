//! The computer grid: room bar, zoom, cards with their status badge, card actions.

use super::*;

/// The small "● SHADOW" badge on each card.
pub(super) struct StatusBadge {
    pub(super) width: f32,
    pub(super) dot: Color32,
    pub(super) icon: Option<(&'static str, Color32)>,
    pub(super) text: std::sync::Arc<egui::Galley>,
    pub(super) fill: Color32,
    pub(super) stroke: Color32,
}

impl StatusBadge {
    pub(super) fn paint(self, painter: &egui::Painter, r: Rect) {
        painter.rect(
            r,
            Rounding::ZERO,
            self.fill,
            Stroke::new(1.0_f32, self.stroke),
        );
        let mut x = r.left() + 9.0;
        painter.rect_filled(
            Rect::from_center_size(pos2(x, r.center().y), vec2(5.0, 5.0)),
            Rounding::ZERO,
            self.dot,
        );
        x += 8.0;
        if let Some((ic, col)) = self.icon {
            painter.text(
                pos2(x, r.center().y),
                Align2::LEFT_CENTER,
                ic,
                FontId::proportional(11.5),
                col,
            );
            x += 15.0;
        }
        let col = self
            .text
            .job
            .sections
            .first()
            .map(|s| s.format.color)
            .unwrap_or(Color32::GRAY);
        painter.galley(
            pos2(x, r.center().y - self.text.size().y / 2.0),
            self.text,
            col,
        );
    }
}

pub(super) fn status_badge(ui: &egui::Ui, p: &Palette, s: &ClientStatus) -> StatusBadge {
    let soft = |c: Color32| c.gamma_multiply(0.75);
    let (icon, label, col): (Option<&'static str>, String, Color32) = match s.freeze {
        FreezeState::Frozen => {
            let pct = s.shield_usage.map(|u| u.percent()).unwrap_or(0);
            let col = if pct >= 90 {
                p.danger
            } else if pct >= 75 {
                p.warning
            } else {
                p.accent_ink
            };
            let label = if pct >= 75 {
                format!("Shadow {pct}%")
            } else {
                "Shadow".into()
            };
            (Some(icons::SNOWFLAKE), label, col)
        }
        FreezeState::FreezePending => (
            Some(icons::SNOWFLAKE),
            "Shadow · kutilmoqda".into(),
            p.warning,
        ),
        FreezeState::ThawPending => (
            Some(icons::SUN_SNOW),
            "Shadow · o'chmoqda".into(),
            p.warning,
        ),
        FreezeState::Maintenance => (Some(icons::WRENCH), "Xizmat".into(), p.warning),
        _ => (None, "Onlayn".into(), p.muted),
    };
    let text = ui_kit::caps(ui, &label, 9.5, ui_kit::mono(), soft(col));
    let width = 9.0 + 8.0 + if icon.is_some() { 15.0 } else { 0.0 } + text.size().x + 9.0;
    StatusBadge {
        width,
        dot: soft(p.success),
        icon: icon.map(|i| (i, soft(col))),
        text,
        fill: p.sunken,
        stroke: p.line,
    }
}

/// Attention icons (alert, help, internet rules, blocked apps, presentation, updates), drawn
/// right-to-left ending at `right`. Returns the x where the next element may end.
fn paint_flags(painter: &egui::Painter, p: &Palette, v: &View, right: f32, y: f32) -> f32 {
    let mut x = right;
    let mut flag = |icon: &str, col: Color32| {
        painter.text(
            pos2(x, y),
            Align2::RIGHT_CENTER,
            icon,
            FontId::proportional(14.0),
            col,
        );
        x -= 22.0;
    };
    if v.alert.is_some() {
        flag(icons::TRIANGLE_ALERT, p.danger);
    }
    if v.help.is_some() {
        flag(icons::HAND, p.warning);
    }
    if v.status.web_blocklist + v.status.keywords > 0 || v.status.safe_search {
        flag(icons::GLOBE_LOCK, p.accent_ink);
    }
    if v.status.app_blocklist > 0 {
        flag(icons::BAN, p.danger);
    }
    if v.status.showing_teacher {
        flag(icons::CAST, p.accent_ink);
    }
    if v.status.busy.is_some() {
        flag(icons::DOWNLOAD, p.warning);
    } else if v.status.os_updates.unwrap_or(0) + v.status.app_updates.unwrap_or(0) > 0 {
        flag(icons::DOWNLOAD, p.muted);
    }
    x
}

const ROW_H: f32 = 44.0;

/// List-view columns: (title, width; 0 = flexible). Narrow windows drop the trailing ones.
fn list_columns(width: f32) -> Vec<(&'static str, f32)> {
    let mut cols = vec![
        ("#", 56.0),
        ("", 74.0),
        ("Kompyuter", 0.0),
        ("Holat", 190.0),
        ("Belgilar", 130.0),
        ("Xona", 140.0),
    ];
    if width > 1000.0 {
        cols.push(("Foydalanuvchi", 140.0));
    }
    if width > 1150.0 {
        cols.push(("IP", 130.0));
    }
    if width > 1250.0 {
        cols.push(("Ulangan", 90.0));
    }
    cols
}

/// Resolves flexible columns: returns each column's x range within `rect`.
fn column_spans(cols: &[(&str, f32)], rect: Rect) -> Vec<(f32, f32)> {
    let fixed: f32 = cols.iter().map(|c| c.1).sum();
    let flex = (rect.width() - fixed).max(120.0);
    let mut x = rect.left();
    cols.iter()
        .map(|c| {
            let w = if c.1 == 0.0 { flex } else { c.1 };
            let span = (x, x + w);
            x += w;
            span
        })
        .collect()
}

impl ConsoleApp {
    fn grid_view(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        visible: &[&View],
        rooms: &[String],
        actions: &mut Vec<Action>,
    ) {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let avail = ui.available_width() - 2.0;
                let cols = self.columns() as usize;
                let w = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
                let h = card_height(w);
                let rows = visible.len().div_ceil(cols);
                let origin = ui.cursor().min;
                let content_h = rows as f32 * (h + GAP);
                let bg_rect = Rect::from_min_size(
                    origin,
                    vec2(avail, content_h.max(ui.clip_rect().bottom() - origin.y)),
                );
                let bg = ui.interact(bg_rect, egui::Id::new("list-bg"), Sense::click_and_drag());

                ui.spacing_mut().item_spacing.y = GAP;
                let mut items: Hitboxes = Vec::with_capacity(visible.len());
                for row in visible.chunks(cols) {
                    let (row_rect, _) = ui.allocate_exact_size(vec2(avail, h), Sense::hover());
                    for (i, v) in row.iter().enumerate() {
                        let rect = Rect::from_min_size(
                            pos2(row_rect.left() + 1.0 + i as f32 * (w + GAP), row_rect.top()),
                            vec2(w, h),
                        );
                        self.card(ui, p, v, rect, rooms, actions);
                        items.push((v.id, rect));
                    }
                }
                // Selection outlines last, so neighbouring cells can't paint over them.
                for (id, r) in &items {
                    if self.selected.contains(id) {
                        ui.painter().rect_stroke(
                            *r,
                            Rounding::ZERO,
                            Stroke::new(2.0_f32, p.accent),
                        );
                    }
                }
                self.marquee(ui, p, &bg, origin, &items);
            });
    }

    fn list_view(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        visible: &[&View],
        rooms: &[String],
        actions: &mut Vec<Action>,
    ) {
        let cols = list_columns(ui.available_width());
        // Header
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        ui.painter().rect_filled(head, Rounding::ZERO, p.sunken);
        ui.painter()
            .rect_stroke(head, Rounding::ZERO, Stroke::new(1.0_f32, p.line));
        for ((title, _), (x0, _)) in cols.iter().zip(column_spans(&cols, head)) {
            let g = ui_kit::caps(ui, title, 9.5, ui_kit::mono(), p.faint);
            ui.painter().galley(
                pos2(x0 + 12.0, head.center().y - g.size().y / 2.0),
                g,
                p.faint,
            );
        }
        ui.add_space(-ui.spacing().item_spacing.y);

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let avail = ui.available_width();
                let origin = ui.cursor().min;
                let content_h = visible.len() as f32 * ROW_H;
                let bg_rect = Rect::from_min_size(
                    origin,
                    vec2(avail, content_h.max(ui.clip_rect().bottom() - origin.y)),
                );
                let bg = ui.interact(bg_rect, egui::Id::new("list-bg"), Sense::click_and_drag());

                ui.spacing_mut().item_spacing.y = 0.0;
                let mut items: Hitboxes = Vec::with_capacity(visible.len());
                for v in visible {
                    let (rect, _) = ui.allocate_exact_size(vec2(avail, ROW_H), Sense::hover());
                    self.list_row(ui, p, v, rect, &cols, rooms, actions);
                    items.push((v.id, rect));
                }
                self.marquee(ui, p, &bg, origin, &items);
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn list_row(
        &self,
        ui: &mut egui::Ui,
        p: &Palette,
        v: &View,
        rect: Rect,
        cols: &[(&str, f32)],
        rooms: &[String],
        actions: &mut Vec<Action>,
    ) {
        let resp = ui.interact(rect, egui::Id::new(("row", v.id)), Sense::click());
        let selected = self.selected.contains(&v.id);
        let painter = ui.painter_at(rect);
        let fill = match (selected, resp.hovered()) {
            (true, _) => p.card.lerp_to_gamma(p.accent, 0.10),
            (false, true) => p.card_hover,
            (false, false) => p.card,
        };
        painter.rect_filled(rect, Rounding::ZERO, fill);
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, p.line),
        );
        painter.vline(
            rect.left() + 0.5,
            rect.y_range(),
            Stroke::new(1.0_f32, p.line),
        );
        painter.vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0_f32, p.line),
        );
        if selected {
            painter.rect_filled(
                Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                Rounding::ZERO,
                p.accent,
            );
        }
        let cy = rect.center().y;
        let spans = column_spans(cols, rect);
        for ((title, _), (x0, x1)) in cols.iter().zip(spans) {
            let inner = Rect::from_x_y_ranges(x0 + 12.0..=x1 - 8.0, rect.y_range());
            match *title {
                "#" => {
                    let num = ui_kit::caps(
                        ui,
                        &format!("{:02}", v.number),
                        ui_kit::HUD,
                        ui_kit::mono_bold(),
                        p.text,
                    );
                    let tag = Rect::from_min_size(pos2(inner.left(), cy - 10.0), vec2(34.0, 20.0));
                    painter.rect_filled(tag, Rounding::ZERO, p.hover);
                    painter.galley(tag.center() - num.size() / 2.0, num, p.text);
                }
                "" => {
                    let thumb =
                        Rect::from_min_size(pos2(inner.left(), cy - 16.0), vec2(56.0, 32.0));
                    painter.rect_filled(thumb, Rounding::ZERO, p.sunken);
                    if let Some((_, tex)) = self.textures.get(&v.id) {
                        let s = tex.size_vec2();
                        let scale = (thumb.width() / s.x).min(thumb.height() / s.y);
                        painter.image(
                            tex.id(),
                            Rect::from_center_size(thumb.center(), s * scale),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    } else {
                        painter.text(
                            thumb.center(),
                            Align2::CENTER_CENTER,
                            icons::MONITOR,
                            FontId::proportional(13.0),
                            p.faint,
                        );
                    }
                    if v.status.locked {
                        painter.rect_filled(thumb, Rounding::ZERO, Color32::from_black_alpha(150));
                        painter.text(
                            thumb.center(),
                            Align2::CENTER_CENTER,
                            icons::LOCK,
                            FontId::proportional(14.0),
                            p.warning,
                        );
                    }
                }
                "Kompyuter" => {
                    let g = ui_kit::truncated(
                        ui,
                        &v.label,
                        FontId::new(14.0, ui_kit::bold()),
                        p.strong,
                        inner.width(),
                    );
                    painter.galley(pos2(inner.left(), cy - g.size().y / 2.0), g, p.strong);
                }
                "Holat" => {
                    let badge = status_badge(ui, p, &v.status);
                    let r =
                        Rect::from_min_size(pos2(inner.left(), cy - 10.0), vec2(badge.width, 20.0));
                    badge.paint(&painter, r);
                }
                "Belgilar" => {
                    let flags_w: f32 = 6.0 * 22.0;
                    paint_flags(
                        &painter,
                        p,
                        v,
                        inner.left() + flags_w.min(inner.width()),
                        cy,
                    );
                }
                _ => {
                    let text = match *title {
                        "Xona" => v.room.clone().unwrap_or_else(|| "—".into()),
                        "Foydalanuvchi" => v.user.clone(),
                        "IP" => v.ip.clone(),
                        _ => fmt_dur(v.online_secs),
                    };
                    let g = ui_kit::truncated(
                        ui,
                        &text,
                        FontId::proportional(13.0),
                        p.muted,
                        inner.width(),
                    );
                    painter.galley(pos2(inner.left(), cy - g.size().y / 2.0), g, p.muted);
                }
            }
        }
        item_interactions(&resp, v, rooms, actions);
    }
}

pub(super) fn empty_state(ui: &mut egui::Ui, p: &Palette, cfg: &ServerConfig) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.2);
        ui.label(ui_kit::icon(icons::WIFI_OFF, 48.0, p.faint));
        ui.add_space(10.0);
        ui.label(RichText::new("Kompyuterlar hali ulanmagan").family(ui_kit::bold()).size(22.0).color(p.strong));
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "O'quvchi kompyuterlarida Vimora agentini ishga tushiring.\nPort {} · konsoldagi bilan bir xil ulanish kaliti",
                cfg.port
            ))
            .color(p.muted),
        );
        ui.add_space(14.0);
        ui.label(RichText::new("  vimora-client --key <ulanish-kaliti>  ").family(ui_kit::mono()).color(p.text).background_color(p.sunken));
    });
}

impl ConsoleApp {
    pub(super) fn grid(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        egui::CentralPanel::default().frame(Frame::none().fill(p.bg).inner_margin(Margin::symmetric(20.0, 18.0))).show(ctx, |ui| {
            let n_sel = all.iter().filter(|v| self.selected.contains(&v.id)).count();

            ui.horizontal(|ui| {
                self.room_bar(ui, p, all);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let view = self.cfg.read().unwrap().view;
                    // Grid / list switch (right-to-left: list first).
                    ui_kit::segmented(p).show(ui, |ui| ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (mode, ic, tip) in [(ViewMode::List, icons::LIST, "Ro'yxat"), (ViewMode::Grid, icons::LAYOUT_GRID, "Katakchalar")] {
                            let on = view == mode;
                            let r = ui_kit::button(ui, Some(ic), "", if on { Kind::Outline } else { Kind::Ghost }).on_hover_text(tip);
                            if r.clicked() {
                                self.cfg.write().unwrap().view = mode;
                            }
                        }
                    }));
                    ui.add_space(10.0);
                    if view == ViewMode::Grid {
                        // Zoom: [-] 100% [+]  (100% = 6 computers per row), one rounded group.
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let cols = self.columns();
                        ui_kit::segmented(p).show(ui, |ui| ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            // Right-to-left parent: add "+" first so it ends up on the right.
                            if ui_kit::button(ui, Some(icons::PLUS), "", Kind::Ghost).on_hover_text("Kattalashtirish (Ctrl + g'ildirak)").clicked() {
                                self.set_columns(cols - 1);
                            }
                            let pct = ui_kit::caps(ui, &format!("{}%", zoom_percent(cols)), ui_kit::HUD, ui_kit::mono_bold(), p.text);
                            let (r, resp) = ui.allocate_exact_size(vec2(58.0, ui_kit::CONTROL_H), Sense::click());
                            ui.painter().galley(r.center() - pct.size() / 2.0, pct, p.text);
                            if resp.on_hover_text(format!("Qatorda {cols} ta · bosing: 100%")).clicked() {
                                self.set_columns(DEFAULT_COLUMNS);
                            }
                            if ui_kit::button(ui, Some(icons::MINUS), "", Kind::Ghost).on_hover_text("Kichiklashtirish (Ctrl + g'ildirak)").clicked() {
                                self.set_columns(cols + 1);
                            }
                        }));
                        ui.add_space(10.0);
                    }

                    ui.add_space(10.0);
                    ui_kit::input_frame(p).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add(ui_kit::text_field(&mut self.search).hint_text("Kompyuterni qidirish").frame(false).desired_width(180.0));
                        ui.label(RichText::new(icons::SEARCH).color(p.faint));
                    });
                });
            });
            ui.add_space(16.0);

            let visible_all: Vec<&View> = all.iter().filter(|v| self.in_tab(v)).collect();
            let _ = n_sel;

            if all.is_empty() {
                empty_state(ui, p, &self.cfg.read().unwrap());
                return;
            }

            // Ctrl + mouse wheel zoom: accumulate the gesture, one column per notch.
            let pointer_in = ui.rect_contains_pointer(ui.available_rect_before_wrap());
            let zd = ctx.input(|i| i.zoom_delta());
            let grid_view = self.cfg.read().unwrap().view == ViewMode::Grid;
            if grid_view && pointer_in && zd != 1.0 && self.dialog.is_none() {
                self.zoom_acc *= zd;
                if self.zoom_acc > 1.12 {
                    self.set_columns(self.columns() - 1);
                    self.zoom_acc = 1.0;
                } else if self.zoom_acc < 0.89 {
                    self.set_columns(self.columns() + 1);
                    self.zoom_acc = 1.0;
                }
            }

            let q = self.search.to_lowercase();
            let visible: Vec<&View> = visible_all.into_iter().filter(|v| q.is_empty() || v.label.to_lowercase().contains(&q)).collect();
            if visible.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui_kit::hud(ui, "Bu ro'yxatda kompyuter yo'q", p.faint);
                    ui.add_space(6.0);
                    ui.label(RichText::new("Kompyuterni o'ng tugma bilan bosib, \"Xonaga biriktirish\" ni tanlang.").color(p.muted));
                });
                return;
            }

            self.visible_order = visible.iter().map(|v| v.id).collect();
            let mut actions = Vec::new();
            let rooms = self.cfg.read().unwrap().rooms.clone();
            if grid_view {
                self.grid_view(ui, p, &visible, &rooms, &mut actions);
            } else {
                self.list_view(ui, p, &visible, &rooms, &mut actions);
            }
            for a in actions {
                self.apply(a, all);
            }
        });
    }

    pub(super) fn card(
        &self,
        ui: &mut egui::Ui,
        p: &Palette,
        v: &View,
        rect: Rect,
        rooms: &[String],
        actions: &mut Vec<Action>,
    ) {
        let resp = ui.interact(rect, egui::Id::new(("card", v.id)), Sense::click());
        let selected = self.selected.contains(&v.id);
        let hovered = resp.hovered();
        let painter = ui.painter_at(rect.expand(2.0));
        let pad = 12.0;

        // Selection = accent border (drawn by the grid) + a faint accent wash.
        let fill = match (selected, hovered) {
            (true, _) => p.card.lerp_to_gamma(p.accent, 0.07),
            (false, true) => p.card_hover,
            (false, false) => p.card,
        };
        painter.rect(
            rect,
            Rounding::ZERO,
            fill,
            Stroke::new(1.0_f32, if hovered { p.line_strong } else { p.line }),
        );

        // Top strip: number tag · status icons · select box
        let strip = Rect::from_min_size(rect.min, vec2(rect.width(), STRIP_H));
        painter.hline(
            strip.x_range(),
            strip.bottom(),
            Stroke::new(1.0_f32, p.line),
        );
        // Every number tag looks the same: equal width, neutral colour.
        let num = ui_kit::caps(
            ui,
            &format!("{:02}", v.number),
            ui_kit::HUD,
            ui_kit::mono_bold(),
            p.text,
        );
        let tag = Rect::from_min_size(
            pos2(rect.left() + pad, strip.center().y - 10.0),
            vec2(34.0, 20.0),
        );
        painter.rect_filled(tag, Rounding::ZERO, p.hover);
        painter.galley(tag.center() - num.size() / 2.0, num, p.text);

        // Status badge (top-right): online dot + Shadow mode. Deliberately low-contrast.
        let badge = status_badge(ui, p, &v.status);
        let chk = Rect::from_min_size(
            pos2(rect.right() - pad - badge.width, strip.center().y - 10.0),
            vec2(badge.width, 20.0),
        );
        badge.paint(&painter, chk);
        // Only states that need attention get an icon.
        paint_flags(&painter, p, v, chk.left() - 12.0, strip.center().y);

        // Thumbnail
        let tw = rect.width() - pad * 2.0;
        let thumb = Rect::from_min_size(
            pos2(rect.left() + pad, strip.bottom() + pad),
            vec2(tw, (tw * 9.0 / 16.0).round()),
        );
        painter.rect(thumb, Rounding::ZERO, p.sunken, Stroke::NONE);
        if let Some((_, tex)) = self.textures.get(&v.id) {
            let s = tex.size_vec2();
            let scale = (thumb.width() / s.x).min(thumb.height() / s.y);
            let r = Rect::from_center_size(thumb.center(), s * scale);
            painter.image(
                tex.id(),
                r,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.rect_stroke(
                thumb.expand(1.0),
                Rounding::ZERO,
                Stroke::new(2.0_f32, if hovered { p.card_hover } else { p.card }),
            );
        } else {
            painter.text(
                thumb.center(),
                Align2::CENTER_CENTER,
                icons::MONITOR,
                FontId::proportional(24.0),
                p.faint,
            );
        }
        if v.status.locked {
            painter.rect_filled(thumb, Rounding::ZERO, Color32::from_black_alpha(150));
            painter.text(
                thumb.center(),
                Align2::CENTER_CENTER,
                icons::LOCK,
                FontId::proportional(28.0),
                p.warning,
            );
        }

        // Name
        let name = ui_kit::truncated(
            ui,
            &v.label,
            FontId::new(15.0, ui_kit::bold()),
            p.strong,
            tw,
        );
        painter.galley(
            pos2(rect.left() + pad, thumb.bottom() + 10.0),
            name,
            p.strong,
        );

        item_interactions(&resp, v, rooms, actions);
    }
}

/// Tooltip text with everything the card/row doesn't show.
fn details(v: &View) -> String {
    let room = v.room.as_deref().unwrap_or("—");
    format!(
        "{}\nFoydalanuvchi: {}\nIP: {}\nTizim: {}\nXona: {room}\nUlangan: {}\nShadow: {}\nYangilanishlar: {}{}{}",
        v.label,
        v.user,
        v.ip,
        v.os,
        fmt_dur(v.online_secs),
        freeze_label(v.status.freeze),
        updates_label(&v.status),
        if v.status.service { "" } else { "\n⚠ Vimora xizmati ishlamayapti" },
        v.alert.as_ref().map(|a| format!("\n⚠ {a}")).unwrap_or_default()
    )
}

/// Click / double-click / context menu, shared by cards and list rows.
fn item_interactions(resp: &egui::Response, v: &View, rooms: &[String], actions: &mut Vec<Action>) {
    if resp.hovered() {
        resp.clone().on_hover_text(details(v));
    }
    if resp.clicked() {
        let mods = resp.ctx.input(|i| i.modifiers);
        actions.push(Action::Click(v.id, mods));
    }
    if resp.double_clicked() {
        actions.push(Action::View(v.id, false));
    }
    {
        resp.context_menu(|ui| {
            for (ic, label, a) in [
                (icons::EYE, "Ko'rish", Action::View(v.id, false)),
                (
                    icons::MOUSE_POINTER_2,
                    "Boshqarish",
                    Action::View(v.id, true),
                ),
                (icons::LOCK, "Qulflash", Action::Lock(v.id)),
                (icons::LOCK_OPEN, "Ochish", Action::Unlock(v.id)),
                (icons::MESSAGE_SQUARE_TEXT, "Xabar", Action::Message(v.id)),
                (icons::SETTINGS, "Nomini o'zgartirish", Action::Rename(v.id)),
            ] {
                if ui.button(with_icon(ic, label)).clicked() {
                    actions.push(a);
                    ui.close_menu();
                }
            }
            ui.separator();
            ui.menu_button(with_icon(icons::LAYOUT_GRID, "Xonaga biriktirish"), |ui| {
                for r in rooms {
                    let current = v.room.as_deref() == Some(r.as_str());
                    if ui.selectable_label(current, r).clicked() {
                        actions.push(Action::Assign(v.id, Some(r.clone())));
                        ui.close_menu();
                    }
                }
                if !rooms.is_empty() {
                    ui.separator();
                }
                if ui.button(with_icon(icons::PLUS, "Yangi xona…")).clicked() {
                    actions.push(Action::NewRoomFor(v.id));
                    ui.close_menu();
                }
                if v.room.is_some()
                    && ui
                        .button(with_icon(icons::X, "Xonadan chiqarish"))
                        .clicked()
                {
                    actions.push(Action::Assign(v.id, None));
                    ui.close_menu();
                }
            });
            if v.help.is_some()
                && ui
                    .button(with_icon(icons::CHECK, "Yordam berildi"))
                    .clicked()
            {
                actions.push(Action::ClearHelp(v.id));
                ui.close_menu();
            }
        });
    }
}

impl ConsoleApp {
    pub(super) fn apply(&mut self, a: Action, all: &[View]) {
        match a {
            Action::Click(id, mods) => self.click_select(id, mods),
            Action::Rename(id) => {
                if let Some(v) = all.iter().find(|v| v.id == id) {
                    let alias = self
                        .cfg
                        .read()
                        .unwrap()
                        .aliases
                        .get(&v.key)
                        .cloned()
                        .unwrap_or_default();
                    self.open_dialog(Dialog::Rename {
                        id,
                        key: v.key.clone(),
                        alias,
                        system: false,
                        system_name: v.hostname.clone(),
                        restart: false,
                    });
                }
            }
            Action::View(id, control) => self.open_viewer(id, control),
            Action::Lock(id) => self.lock(&[id]),
            Action::Unlock(id) => self.send(&[id], Command::Unlock, "Qulfdan chiqarish"),
            Action::Message(id) => {
                self.selected = [id].into();
                self.open_dialog(Dialog::Message {
                    text: String::new(),
                    send: false,
                });
            }
            Action::ClearHelp(id) => {
                if let Some(s) = self.hub.lock().unwrap().students.get_mut(&id) {
                    s.help = None;
                }
                self.send(&[id], Command::ClearHelp, "");
            }
            Action::Assign(id, room) => {
                let keys = self.assign_keys(id, all);
                self.assign(&keys, room);
            }
            Action::NewRoomFor(id) => {
                self.pending_assign = self.assign_keys(id, all);
                self.open_dialog(Dialog::Room {
                    name: String::new(),
                    rename: None,
                });
            }
        }
    }
}
