//! The computer grid: cards with thumbnails and status.

use super::*;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

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

/// Badges of one computer, left to right: always "online", plus Shadow / maintenance when
/// that applies (they are independent states).
pub(super) fn status_badges(ui: &egui::Ui, p: &Palette, s: &ClientStatus) -> Vec<StatusBadge> {
    let soft = |c: Color32| c.gamma_multiply(0.75);
    let make = |icon: Option<&'static str>, label: &str, col: Color32| {
        let text = ui_kit::caps(ui, label, 9.5, ui_kit::mono(), soft(col));
        let width = 9.0 + 8.0 + if icon.is_some() { 15.0 } else { 0.0 } + text.size().x + 9.0;
        StatusBadge {
            width,
            dot: soft(p.success),
            icon: icon.map(|i| (i, soft(col))),
            text,
            fill: p.sunken,
            stroke: p.line,
        }
    };
    let mut out = vec![make(None, tr("Onlayn"), p.muted)];
    let extra: Option<(&'static str, String, Color32)> = match s.freeze {
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
                tf!("Shadow {pct}%", pct = pct)
            } else {
                tr("Shadow").into()
            };
            Some((icons::SNOWFLAKE, label, col))
        }
        FreezeState::FreezePending => Some((
            icons::SNOWFLAKE,
            tr("Shadow · kutilmoqda").into(),
            p.warning,
        )),
        FreezeState::ThawPending => {
            Some((icons::SUN_SNOW, tr("Shadow · o'chmoqda").into(), p.warning))
        }
        FreezeState::Maintenance => Some((icons::WRENCH, tr("Xizmat").into(), p.warning)),
        _ => None,
    };
    if let Some((ic, label, col)) = extra {
        out.push(make(Some(ic), &label, col));
    }
    out
}

/// The "online" badge alone.
fn online_badge(ui: &egui::Ui, p: &Palette, s: &ClientStatus) -> StatusBadge {
    status_badges(ui, p, s)
        .into_iter()
        .next()
        .expect("online badge")
}

/// The Shadow / maintenance badge: its own thing, independent of the online status.
fn shadow_badge(ui: &egui::Ui, p: &Palette, s: &ClientStatus) -> Option<StatusBadge> {
    status_badges(ui, p, s).into_iter().nth(1)
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

const ROW_H: f32 = 36.0;

/// List-view columns: (column, width; 0 = flexible = the last one). Narrow windows drop trailing ones.
fn list_columns(width: f32) -> Vec<(Col, f32)> {
    let mut cols = vec![
        (Col::Num, 52.0),
        (Col::Thumb, 62.0),
        (Col::Name, 140.0),
        (Col::Status, 100.0),
        (Col::Shadow, 140.0),
        (Col::Flags, 120.0),
        (Col::Room, 130.0),
    ];
    if width > 1000.0 {
        cols.push((Col::User, 140.0));
    }
    if width > 1150.0 {
        cols.push((Col::Ip, 130.0));
    }
    if width > 1250.0 {
        cols.push((Col::Online, 90.0));
    }
    if let Some(last) = cols.last_mut() {
        last.1 = 0.0;
    }
    cols
}

/// Resolves flexible columns: returns each column's x range within `rect`.
fn column_spans(cols: &[(Col, f32)], rect: Rect) -> Vec<(f32, f32)> {
    let fixed: f32 = cols.iter().map(|c| c.1).sum();
    let flex = (rect.width() - fixed).max(90.0);
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
        let any_filter = self.list_filter.any();
        for ((col, _), (x0, x1)) in cols.iter().zip(column_spans(&cols, head)) {
            let cell = Rect::from_x_y_ranges(x0..=x1, head.y_range());
            let active = self.list_filter.active(*col);
            // "#" doubles as "clear all filters" while any filter is on.
            if *col == Col::Num && any_filter {
                let resp = ui.interact(cell, egui::Id::new("col-clear"), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(cell, Rounding::ZERO, p.hover);
                }
                ui.painter().text(
                    cell.center(),
                    Align2::CENTER_CENTER,
                    icons::X,
                    FontId::proportional(13.0),
                    p.accent_ink,
                );
                if resp.tip(tr("Tozalash")).clicked() {
                    self.list_filter = ListFilter::default();
                }
                continue;
            }
            let id = egui::Id::new(("col-filter", *col as u8));
            let resp = col
                .filterable()
                .then(|| ui.interact(cell, id.with("hdr"), Sense::click()));
            if resp.as_ref().is_some_and(|r| r.hovered()) {
                ui.painter().rect_filled(cell, Rounding::ZERO, p.hover);
            }
            let ink = if active { p.accent_ink } else { p.faint };
            let g = ui_kit::caps(ui, col.title(), 9.5, ui_kit::mono(), ink);
            let gw = g.size().x;
            ui.painter()
                .galley(pos2(x0 + 8.0, head.center().y - g.size().y / 2.0), g, ink);
            let Some(resp) = resp else { continue };
            ui.painter().text(
                pos2(x0 + 8.0 + gw + 6.0, head.center().y),
                Align2::LEFT_CENTER,
                icons::FILTER,
                FontId::proportional(11.0),
                if active {
                    p.accent_ink
                } else {
                    p.faint.gamma_multiply(0.6)
                },
            );
            let resp = resp.tip(tr("Filtr"));
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            let f = &mut self.list_filter;
            egui::popup_below_widget(
                ui,
                id,
                &resp,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(190.0);
                    match col {
                        Col::Name | Col::User | Col::Ip => {
                            let text = match col {
                                Col::Name => &mut f.name,
                                Col::User => &mut f.user,
                                _ => &mut f.ip,
                            };
                            let r = ui.add(
                                ui_kit::text_field(text)
                                    .hint_text(tr("Qidirish…"))
                                    .desired_width(f32::INFINITY),
                            );
                            if ui.memory(|m| m.focused().is_none()) {
                                r.request_focus();
                            }
                        }
                        Col::Status => {
                            for (v, label) in [
                                (StatusFilter::Any, tr("Hammasi")),
                                (StatusFilter::Locked, tr("Qulflangan")),
                                (StatusFilter::Attention, tr("Diqqat talab qiladi")),
                            ] {
                                if ui.selectable_label(f.status == v, label).clicked() {
                                    f.status = v;
                                    ui.close_menu();
                                }
                            }
                        }
                        Col::Shadow => {
                            for (v, label) in [
                                (ShadowFilter::Any, tr("Hammasi")),
                                (ShadowFilter::On, tr("Shadow yoqilgan")),
                                (ShadowFilter::Off, tr("Shadow o'chiq")),
                            ] {
                                if ui.selectable_label(f.shadow == v, label).clicked() {
                                    f.shadow = v;
                                    ui.close_menu();
                                }
                            }
                        }
                        Col::Room => {
                            if ui
                                .selectable_label(f.room == RoomFilter::Any, tr("Hammasi"))
                                .clicked()
                            {
                                f.room = RoomFilter::Any;
                                ui.close_menu();
                            }
                            if ui
                                .selectable_label(f.room == RoomFilter::Unassigned, tr("Xonasiz"))
                                .clicked()
                            {
                                f.room = RoomFilter::Unassigned;
                                ui.close_menu();
                            }
                            for r in rooms {
                                let on = f.room == RoomFilter::Is(r.clone());
                                if ui.selectable_label(on, r).clicked() {
                                    f.room = RoomFilter::Is(r.clone());
                                    ui.close_menu();
                                }
                            }
                        }
                        _ => {}
                    }
                },
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
        cols: &[(Col, f32)],
        _rooms: &[String],
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
        for ((col, _), (x0, x1)) in cols.iter().zip(spans) {
            let inner = Rect::from_x_y_ranges(x0 + 8.0..=x1 - 6.0, rect.y_range());
            match *col {
                Col::Num => {
                    let num = ui_kit::caps(
                        ui,
                        &format!("{:02}", v.number),
                        12.0,
                        ui_kit::mono_bold(),
                        p.on_accent,
                    );
                    let tag = Rect::from_min_size(pos2(inner.left(), cy - 11.0), vec2(34.0, 22.0));
                    painter.rect_filled(tag, Rounding::ZERO, p.accent);
                    painter.galley(tag.center() - num.size() / 2.0, num, p.on_accent);
                }
                Col::Thumb => {
                    let thumb =
                        Rect::from_min_size(pos2(inner.left(), cy - 14.0), vec2(50.0, 28.0));
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
                Col::Name => {
                    let g = ui_kit::truncated(
                        ui,
                        &v.label,
                        FontId::new(14.0, ui_kit::bold()),
                        p.strong,
                        inner.width(),
                    );
                    painter.galley(pos2(inner.left(), cy - g.size().y / 2.0), g, p.strong);
                }
                Col::Status => {
                    let badge = online_badge(ui, p, &v.status);
                    let w = badge.width;
                    badge.paint(
                        &painter,
                        Rect::from_min_size(pos2(inner.left(), cy - 10.0), vec2(w, 20.0)),
                    );
                }
                Col::Shadow => {
                    if let Some(badge) = shadow_badge(ui, p, &v.status) {
                        let w = badge.width;
                        badge.paint(
                            &painter,
                            Rect::from_min_size(pos2(inner.left(), cy - 10.0), vec2(w, 20.0)),
                        );
                    }
                }
                Col::Flags => {
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
                    let text = match *col {
                        Col::Room => v.room.clone().unwrap_or_else(|| "—".into()),
                        Col::User => v.user.clone(),
                        Col::Ip => v.ip.clone(),
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
        item_interactions(&resp, v, actions, false);
    }
}

pub(super) fn empty_state(ui: &mut egui::Ui, p: &Palette, _cfg: &ServerConfig) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.2);
        ui.label(ui_kit::icon(icons::WIFI_OFF, 48.0, p.faint));
        ui.add_space(10.0);
        ui.label(RichText::new(tr("Kompyuterlar hali ulanmagan")).family(ui_kit::bold()).size(22.0).color(p.strong));
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr("O'quvchi kompyuterlariga Vimora'ni o'rnating — ular shu yerda o'zi paydo bo'ladi.\nHech narsa sozlash shart emas: bir xil tarmoqda (Wi-Fi yoki LAN) bo'lsa kifoya."))
                .color(p.muted),
        );
    });
}

impl ConsoleApp {
    pub(super) fn grid(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        // The list is a table that touches the window edges; cards keep a margin.
        let margin = if self.cfg.read().unwrap().view == ViewMode::List {
            Margin::ZERO
        } else {
            Margin::symmetric(10.0, 10.0)
        };
        egui::CentralPanel::default()
            .frame(Frame::none().fill(p.bg).inner_margin(margin))
            .show(ctx, |ui| {
                let visible_all: Vec<&View> = all.iter().filter(|v| self.in_tab(v)).collect();

                if all.is_empty() {
                    empty_state(ui, p, &self.cfg.read().unwrap());
                    return;
                }

                // Ctrl + mouse wheel zoom
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
                let visible: Vec<&View> = visible_all
                    .into_iter()
                    .filter(|v| q.is_empty() || v.label.to_lowercase().contains(&q))
                    .filter(|v| grid_view || self.list_filter.matches(v))
                    .collect();

                if visible.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui_kit::hud(ui, tr("Bu ro'yxatda kompyuter yo'q"), p.faint);
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(tr(
                                "Kompyuterni o'ng tugma bilan bosib, \"Xonaga biriktirish\" ni tanlang.",
                            ))
                            .color(p.muted),
                        );
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
        _rooms: &[String],
        actions: &mut Vec<Action>,
    ) {
        let resp = ui.interact(rect, egui::Id::new(("card", v.id)), Sense::click());
        let selected = self.selected.contains(&v.id);
        let hovered = resp.hovered();
        let painter = ui.painter_at(rect.expand(2.0));
        let round = Rounding::ZERO;

        // Card fill + border
        let fill = match (selected, hovered) {
            (true, _) => p.card.lerp_to_gamma(p.accent, 0.09),
            (false, true) => p.card_hover,
            (false, false) => p.card,
        };
        painter.rect(
            rect,
            round,
            fill,
            Stroke::new(
                if selected { 2.0_f32 } else { 1.0_f32 },
                if selected {
                    p.accent
                } else if hovered {
                    p.line_strong
                } else {
                    p.line
                },
            ),
        );

        let pad = CARD_PAD;

        // ── Thumbnail ──────────────────────────────────────────────────────
        let tw = rect.width() - pad * 2.0;
        let th = (tw * 9.0 / 16.0).round();
        let thumb = Rect::from_min_size(pos2(rect.left() + pad, rect.top() + pad), vec2(tw, th));
        let thumb_round = Rounding::ZERO;
        painter.rect_filled(thumb, thumb_round, p.sunken);

        if let Some((_, tex)) = self.textures.get(&v.id) {
            let s = tex.size_vec2();
            let scale = (thumb.width() / s.x).min(thumb.height() / s.y);
            let tr_ = Rect::from_center_size(thumb.center(), s * scale);
            painter.image(
                tex.id(),
                tr_,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(
                thumb.center(),
                Align2::CENTER_CENTER,
                icons::MONITOR,
                FontId::proportional(26.0),
                p.faint,
            );
        }

        // Lock overlay
        if v.status.locked {
            painter.rect_filled(thumb, thumb_round, Color32::from_black_alpha(160));
            painter.text(
                thumb.center(),
                Align2::CENTER_CENTER,
                icons::LOCK,
                FontId::proportional(28.0),
                p.warning,
            );
        }

        // Flags (top-right of thumbnail, over the image)
        let flags_y = thumb.top() + 15.0;
        let flags_x = thumb.right() - 8.0;
        paint_flags(&painter, p, v, flags_x, flags_y);

        // Number tag (top-left of thumbnail): solid, readable on any screen
        let num = ui_kit::caps(
            ui,
            &format!("{:02}", v.number),
            12.0,
            ui_kit::mono_bold(),
            p.on_accent,
        );
        let tag = Rect::from_min_size(pos2(thumb.left(), thumb.top()), vec2(34.0, 22.0));
        painter.rect_filled(tag, Rounding::ZERO, p.accent);
        painter.galley(tag.center() - num.size() / 2.0, num, p.on_accent);

        // ── Bottom info strip ──────────────────────────────────────────────
        let info_top = thumb.bottom() + 6.0;

        // Name
        let name = ui_kit::truncated(
            ui,
            &v.label,
            FontId::new(13.5, ui_kit::bold()),
            p.strong,
            tw,
        );
        painter.galley(pos2(rect.left() + pad, info_top), name, p.strong);

        // Online status on the left, Shadow (a separate thing) on the right of the same line
        let y = info_top + 22.0;
        let online = online_badge(ui, p, &v.status);
        let w = online.width;
        online.paint(
            &painter,
            Rect::from_min_size(pos2(rect.left() + pad, y), vec2(w, 18.0)),
        );
        if let Some(shadow) = shadow_badge(ui, p, &v.status) {
            let w = shadow.width;
            shadow.paint(
                &painter,
                Rect::from_min_size(pos2(rect.right() - pad - w, y), vec2(w, 18.0)),
            );
        }

        item_interactions(&resp, v, actions, self.ctx_menu.is_none());
    }
}

/// Tooltip text with everything the card/row doesn't show.
fn details(v: &View) -> String {
    let room = v.room.as_deref().unwrap_or("—");
    tf!("{v0}\nFoydalanuvchi: {v1}\nIP: {v2}\nTizim: {v3}\nXona: {room}\nUlangan: {v4}\nShadow: {v5}\nYangilanishlar: {v6}{v7}{v8}", v0 = v.label, v1 = v.user, v2 = v.ip, v3 = v.os, room = room, v4 = fmt_dur(v.online_secs), v5 = freeze_label(v.status.freeze), v6 = updates_label(&v.status), v7 = if v.status.service { "" } else { "\n⚠ Vimora xizmati ishlamayapti" }, v8 = v.alert.as_ref().map(|a| format!("\n⚠ {a}")).unwrap_or_default())
}

/// Click / context menu, shared by cards and list rows.
fn item_interactions(resp: &egui::Response, v: &View, actions: &mut Vec<Action>, tooltip: bool) {
    // Details tooltip: cards only (the list shows the same data in its columns).
    if tooltip {
        ui_kit::tip_at_pointer(resp, &details(v));
    }
    if resp.clicked() {
        let mods = resp.ctx.input(|i| i.modifiers);
        actions.push(Action::Click(v.id, mods));
    }
    if resp.secondary_clicked() {
        let x = resp
            .ctx
            .input(|i| i.pointer.interact_pos())
            .map(|p| p.x)
            .unwrap_or(resp.rect.center().x);
        actions.push(Action::OpenMenu(v.id, resp.rect, x));
    }
}

/// One row of a context menu: icon, label, optional trailing glyph. Returns the response.
fn menu_row(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    trailing: Option<&str>,
    ink: Option<Color32>,
    checked: bool,
) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, Rounding::ZERO, p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let col = ink.unwrap_or(if resp.hovered() { p.strong } else { p.text });
    ui.painter().text(
        pos2(r.left() + 20.0, r.center().y),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(16.0),
        ink.unwrap_or(p.muted),
    );
    let g = ui_kit::truncated(
        ui,
        label,
        FontId::new(13.0, ui_kit::medium()),
        if checked { p.accent_ink } else { col },
        r.width() - 40.0 - 34.0,
    );
    ui.painter().galley(
        pos2(r.left() + 40.0, r.center().y - g.size().y / 2.0),
        g,
        col,
    );
    if checked {
        ui.painter().text(
            pos2(r.right() - 20.0, r.center().y),
            Align2::CENTER_CENTER,
            icons::CHECK,
            FontId::proportional(15.0),
            p.accent_ink,
        );
    } else if let Some(t) = trailing {
        ui.painter().text(
            pos2(r.right() - 20.0, r.center().y),
            Align2::CENTER_CENTER,
            t,
            FontId::proportional(15.0),
            p.faint,
        );
    }
    resp
}

fn menu_sep(ui: &mut egui::Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter()
        .hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, p.line));
}

impl ConsoleApp {
    /// The right-click menu: an arrow popup that points at the computer / room it belongs to.
    pub(super) fn context_menu(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let Some(m) = self.ctx_menu.clone() else {
            return;
        };
        // The computer went away: nothing to act on.
        if let CtxTarget::Computer(id) = &m.target {
            if !all.iter().any(|v| v.id == *id) {
                self.ctx_menu = None;
                return;
            }
        }
        let rooms = self.cfg.read().unwrap().rooms.clone();
        let mut actions: Vec<Action> = vec![];
        let mut room_cmd: Option<(bool, String)> = None; // (delete?, name)
        let mut toggle_rooms = false;
        let mut close = false;

        let rect = ui_kit::arrow_popup(ctx, egui::Id::new("ctx-menu"), m.anchor, m.x, true, |ui| {
            ui.set_width(236.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            match &m.target {
                CtxTarget::Computer(id) => {
                    let Some(v) = all.iter().find(|v| v.id == *id) else {
                        return;
                    };
                    let id = *id;
                    for (ic, label, a) in [
                        (icons::EYE, tr("Ko'rish"), Action::View(id, false)),
                        (
                            icons::MOUSE_POINTER_2,
                            tr("Boshqarish"),
                            Action::View(id, true),
                        ),
                        (icons::LOCK, tr("Qulflash"), Action::Lock(id)),
                        (icons::LOCK_OPEN, tr("unlock|Ochish"), Action::Unlock(id)),
                        (icons::MESSAGE_SQUARE_TEXT, tr("Xabar"), Action::Message(id)),
                        (
                            icons::SETTINGS,
                            tr("Nomini o'zgartirish"),
                            Action::Rename(id),
                        ),
                    ] {
                        if menu_row(ui, p, ic, label, None, None, false).clicked() {
                            actions.push(a);
                            close = true;
                        }
                    }
                    menu_sep(ui, p);
                    let arrow = if m.rooms_open {
                        icons::CHEVRON_DOWN
                    } else {
                        icons::CHEVRON_RIGHT
                    };
                    if menu_row(
                        ui,
                        p,
                        icons::LAYOUT_GRID,
                        tr("Xonaga biriktirish"),
                        Some(arrow),
                        None,
                        false,
                    )
                    .clicked()
                    {
                        toggle_rooms = true;
                    }
                    if m.rooms_open {
                        for r in &rooms {
                            let current = v.room.as_deref() == Some(r.as_str());
                            if menu_row(ui, p, icons::USERS, r, None, None, current).clicked() {
                                actions.push(Action::Assign(id, Some(r.clone())));
                                close = true;
                            }
                        }
                        if menu_row(ui, p, icons::PLUS, tr("Yangi xona…"), None, None, false)
                            .clicked()
                        {
                            actions.push(Action::NewRoomFor(id));
                            close = true;
                        }
                        if v.room.is_some()
                            && menu_row(ui, p, icons::X, tr("Xonadan chiqarish"), None, None, false)
                                .clicked()
                        {
                            actions.push(Action::Assign(id, None));
                            close = true;
                        }
                    }
                    if v.help.is_some() {
                        menu_sep(ui, p);
                        if menu_row(
                            ui,
                            p,
                            icons::CHECK,
                            tr("Yordam berildi"),
                            None,
                            Some(p.success),
                            false,
                        )
                        .clicked()
                        {
                            actions.push(Action::ClearHelp(id));
                            close = true;
                        }
                    }
                }
                CtxTarget::Room(name) => {
                    if menu_row(
                        ui,
                        p,
                        icons::SETTINGS,
                        tr("Nomini o'zgartirish"),
                        None,
                        None,
                        false,
                    )
                    .clicked()
                    {
                        room_cmd = Some((false, name.clone()));
                        close = true;
                    }
                    menu_sep(ui, p);
                    if menu_row(
                        ui,
                        p,
                        icons::TRASH_2,
                        tr("Xonani o'chirish"),
                        None,
                        Some(p.danger),
                        false,
                    )
                    .clicked()
                    {
                        room_cmd = Some((true, name.clone()));
                        close = true;
                    }
                }
            }
        });

        // Close on Esc or a press outside (but not on the press that opened it).
        let (esc, pressed_outside) = ctx.input(|i| {
            (
                i.key_pressed(Key::Escape),
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|pos| !rect.contains(pos)),
            )
        });
        if (esc || pressed_outside) && self.now_frame > m.frame {
            close = true;
        }
        if toggle_rooms {
            if let Some(cm) = &mut self.ctx_menu {
                cm.rooms_open = !cm.rooms_open;
            }
        }
        for a in actions {
            self.apply(a, all);
        }
        if let Some((delete, name)) = room_cmd {
            if delete {
                self.delete_room(&name);
            } else {
                self.open_dialog(Dialog::Room {
                    name: name.clone(),
                    rename: Some(name),
                });
            }
        }
        // An action that opened a dialog / another menu keeps its own state.
        if close && self.ctx_menu.as_ref().is_some_and(|cm| cm.frame == m.frame) {
            self.ctx_menu = None;
        }
    }
}

impl ConsoleApp {
    pub(super) fn apply(&mut self, a: Action, all: &[View]) {
        match a {
            Action::Click(id, mods) => {
                // Double click = two clicks on the SAME computer in quick succession
                // (egui's own detection also fires when the two clicks hit different cards).
                let now = Instant::now();
                let again = self.last_click.is_some_and(|(prev, t)| {
                    prev == id && t.elapsed() < Duration::from_millis(400)
                });
                if again {
                    self.last_click = None;
                    self.open_viewer(id, false);
                } else {
                    self.last_click = Some((id, now));
                    self.click_select(id, mods);
                }
            }
            Action::OpenMenu(id, anchor, x) => {
                self.ctx_menu = Some(CtxMenu {
                    target: CtxTarget::Computer(id),
                    anchor,
                    x,
                    frame: self.now_frame,
                    rooms_open: false,
                });
            }
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
            Action::Unlock(id) => self.send(&[id], Command::Unlock, tr("Qulfdan chiqarish")),
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
