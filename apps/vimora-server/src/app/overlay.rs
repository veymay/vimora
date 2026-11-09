//! Modal / drawer framework with a blurred backdrop, and what confirming each dialog does.

use super::*;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

/// Open/close animation length for modals and drawers.
const OVERLAY_ANIM_SECS: f32 = 0.22;
/// Overlay header and footer bar heights.
const HEADER_H: f32 = 52.0;
const FOOTER_H: f32 = 62.0;

/// Current system hostname of a connected computer.
fn all_hostname(hub: &SharedHub, id: u64) -> Option<String> {
    hub.lock()
        .unwrap()
        .students
        .get(&id)
        .map(|s| s.info.hostname.clone())
}

/// Cheap "frosted glass": downscale 8x by averaging, then a few box-blur passes.
/// Drawn stretched with linear filtering, it reads as a smooth blur.
pub(super) fn blur(img: &egui::ColorImage) -> egui::ColorImage {
    const F: usize = 8;
    let (w, h) = (img.size[0], img.size[1]);
    let (sw, sh) = ((w / F).max(1), (h / F).max(1));
    let mut px = vec![[0f32; 3]; sw * sh];
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = [0f32; 3];
            for dy in 0..F {
                for dx in 0..F {
                    let c = img.pixels[((y * F + dy).min(h - 1)) * w + (x * F + dx).min(w - 1)];
                    acc[0] += c.r() as f32;
                    acc[1] += c.g() as f32;
                    acc[2] += c.b() as f32;
                }
            }
            let n = (F * F) as f32;
            px[y * sw + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
        }
    }
    let r = 2i32;
    let pass = |px: &mut Vec<[f32; 3]>, horizontal: bool| {
        let src = px.clone();
        for y in 0..sh {
            for x in 0..sw {
                let mut acc = [0f32; 3];
                let mut n = 0.0;
                for d in -r..=r {
                    let (xx, yy) = if horizontal {
                        ((x as i32 + d).clamp(0, sw as i32 - 1) as usize, y)
                    } else {
                        (x, (y as i32 + d).clamp(0, sh as i32 - 1) as usize)
                    };
                    let c = src[yy * sw + xx];
                    acc = [acc[0] + c[0], acc[1] + c[1], acc[2] + c[2]];
                    n += 1.0;
                }
                px[y * sw + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
            }
        }
    };
    for _ in 0..3 {
        pass(&mut px, true);
        pass(&mut px, false);
    }
    let pixels = px
        .iter()
        .map(|c| Color32::from_rgb(c[0] as u8, c[1] as u8, c[2] as u8))
        .collect();
    egui::ColorImage {
        size: [sw, sh],
        pixels,
    }
}

impl ConsoleApp {
    /// Opens a modal/drawer after grabbing a snapshot of the console for the blurred backdrop.
    pub(super) fn open_dialog(&mut self, d: Dialog) {
        if self.dialog.is_some() {
            let leaving_chat = matches!(self.dialog, Some(Dialog::Message { .. }))
                && !matches!(d, Dialog::Message { .. });
            if leaving_chat {
                self.hub.lock().unwrap().end_chat();
            }
            self.dialog = Some(d); // switching overlays: keep the current backdrop
            return;
        }
        self.pending = Some((d, Instant::now()));
        self.want_snapshot = true;
    }

    /// Closes the open overlay; it keeps being drawn while it animates out.
    pub(super) fn close_dialog(&mut self) {
        if matches!(self.dialog, Some(Dialog::Message { .. })) {
            // Closing the chat ends it on the students' screens too.
            self.hub.lock().unwrap().end_chat();
        }
        self.closing = self.dialog.take();
        self.pending_assign.clear();
    }

    /// Handles snapshots: dev screenshots and blurred backdrops.
    pub(super) fn snapshots(&mut self, ctx: &egui::Context) {
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            if self.dev_shot {
                if let Some((path, _)) = &self.screenshot {
                    let rgb: Vec<u8> = img
                        .pixels
                        .iter()
                        .flat_map(|c| [c.r(), c.g(), c.b()])
                        .collect();
                    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                    if let Ok(f) = std::fs::File::create(path) {
                        let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(f, 90).encode(
                            &rgb,
                            w,
                            h,
                            image::ExtendedColorType::Rgb8,
                        );
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if let Some((d, _)) = self.pending.take() {
                self.backdrop =
                    Some(ctx.load_texture("backdrop", blur(&img), TextureOptions::LINEAR));
                self.dialog = Some(d);
            }
        }
        // Never wait long for the snapshot: open with a plain dim instead.
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_millis(350))
        {
            self.dialog = self.pending.take().map(|(d, _)| d);
        }
        if self.pending.is_some() {
            ctx.request_repaint();
        }
    }

    /// Result messages (command outcomes) stacked at the bottom right, above the status bar.
    pub(super) fn toasts(&mut self, ctx: &egui::Context, p: &Palette) {
        let now = Instant::now();
        let items: Vec<(bool, String, Duration)> = {
            let mut h = self.hub.lock().unwrap();
            h.toasts.retain(|t| {
                now.duration_since(t.at) < Duration::from_secs(if t.ok { 6 } else { 12 })
            });
            h.toasts
                .iter()
                .map(|t| (t.ok, t.text.clone(), now.duration_since(t.at)))
                .collect()
        };
        if items.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        let screen = ctx.screen_rect();
        let w = 380.0_f32.min(screen.width() - 24.0);
        let mut dismiss: Option<usize> = None;
        let mut y = screen.bottom() - super::navbar::STATUS_H - 12.0;
        for (i, (ok, text, age)) in items.iter().enumerate().rev() {
            let col = if *ok { p.accent } else { p.danger };
            let fade = (age.as_secs_f32() / 0.18).min(1.0);
            let r = egui::Area::new(egui::Id::new(("toast", i)))
                .order(egui::Order::Tooltip)
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(pos2(screen.right() - 12.0, y))
                .show(ctx, |ui| {
                    ui.set_opacity(fade);
                    Frame::none()
                        .fill(p.card)
                        .stroke(Stroke::new(1.0_f32, p.line_strong))
                        .shadow(egui::epaint::Shadow {
                            offset: vec2(0.0, 6.0),
                            blur: 20.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(if p.dark { 110 } else { 35 }),
                        })
                        .show(ui, |ui| {
                            ui.set_width(w);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let (bar, _) =
                                    ui.allocate_exact_size(vec2(4.0, 44.0), Sense::hover());
                                ui.painter().rect_filled(bar, Rounding::ZERO, col);
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(if *ok {
                                        icons::CIRCLE_CHECK
                                    } else {
                                        icons::TRIANGLE_ALERT
                                    })
                                    .size(16.0)
                                    .color(col),
                                );
                                ui.add_space(10.0);
                                ui.add_sized(
                                    vec2(w - 58.0, 44.0),
                                    egui::Label::new(RichText::new(text).size(13.0).color(p.text))
                                        .wrap(),
                                );
                            });
                        });
                })
                .response;
            y = r.rect.top() - 8.0;
            if r.interact(Sense::click()).clicked() {
                dismiss = Some(i);
            }
        }
        if let Some(i) = dismiss {
            self.hub.lock().unwrap().toasts.remove(i);
        }
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context, p: &Palette, all_views: &[View]) {
        // Forms only deal with connected computers.
        let connected: Vec<View> = if self.dialog.is_some() || self.closing.is_some() {
            all_views.iter().filter(|v| v.online).cloned().collect()
        } else {
            vec![]
        };
        let all: &[View] = &connected;
        // 0 → 1 while opening, 1 → 0 while closing; eased below.
        let live = self.dialog.is_some();
        let t = ctx.animate_bool_with_time(egui::Id::new("overlay-anim"), live, OVERLAY_ANIM_SECS);
        if live {
            self.closing = None;
        } else if self.closing.is_none() || t <= 0.0 {
            self.closing = None;
            self.backdrop = None;
            return;
        }
        let ease = 1.0 - (1.0 - t).powi(3); // ease-out cubic
                                            // The Shadow drawer acts on the computers ticked in its own list.
        let targets = match self.dialog.as_ref().or(self.closing.as_ref()) {
            Some(Dialog::Freeze { chosen, .. }) => all
                .iter()
                .map(|v| v.id)
                .filter(|id| chosen.contains(id))
                .collect(),
            _ => self.targets(all),
        };
        let n = targets.len();
        // Power window "Yoqish": the offline computers it would wake (selected ones, else the
        // offline ones of the current tab), and how many of them have a known address.
        let wake_keys: Vec<String> = if matches!(
            self.dialog.as_ref().or(self.closing.as_ref()),
            Some(Dialog::Power(PowerChoice::Wake))
        ) {
            let sel: Vec<&View> = all_views
                .iter()
                .filter(|v| !v.online && self.selected.contains(&v.id))
                .collect();
            let list: Vec<&View> = if sel.is_empty() {
                all_views
                    .iter()
                    .filter(|v| !v.online && self.in_tab(v))
                    .collect()
            } else {
                sel
            };
            list.iter().map(|v| v.key.clone()).collect()
        } else {
            vec![]
        };
        let offline = wake_keys.len();
        let offline_with_mac = {
            let c = self.cfg.read().unwrap();
            wake_keys
                .iter()
                .filter(|k| wake_mac(&c, k).is_some())
                .count()
        };
        let recent: Vec<String> = self.cfg.read().unwrap().recent_urls.clone();
        let hub = self.hub.clone();
        let cfg = self.cfg.clone();
        let screen = ctx.screen_rect();
        let settings_open = matches!(self.dialog, Some(Dialog::Settings(_)));
        let fingerprint = if settings_open {
            vimora_core::auth::fingerprint(&hub.lock().unwrap().identity.public())
        } else {
            String::new()
        };
        let scx = SettingsCtx {
            fingerprint: &fingerprint,
            phase: if settings_open {
                self.updater.phase()
            } else {
                Default::default()
            },
            online: all.len(),
            saved: self
                .saved_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2)),
        };
        let mut settings_out = SettingsOut::default();
        let mut settings_before: Option<ServerConfig> = None;

        // Backdrop: blurred snapshot of the console (or a plain dim until it's ready).
        let backdrop_clicked = egui::Area::new(egui::Id::new("backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.set_opacity(ease);
                let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                if let Some(t) = &self.backdrop {
                    ui.painter().image(
                        t.id(),
                        r,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.painter().rect_filled(
                        r,
                        Rounding::ZERO,
                        Color32::from_black_alpha(if p.dark { 70 } else { 25 }),
                    );
                } else {
                    ui.painter()
                        .rect_filled(r, Rounding::ZERO, Color32::from_black_alpha(130));
                }
                resp.clicked()
            })
            .inner;

        let Some(dialog) = (if live {
            self.dialog.as_mut()
        } else {
            self.closing.as_mut()
        }) else {
            return;
        };
        let drawer = matches!(
            dialog,
            Dialog::Net { .. }
                | Dialog::Message { .. }
                | Dialog::Apps(_)
                | Dialog::Activity
                | Dialog::Freeze { .. }
                | Dialog::Updates { .. }
        );
        let (title, width) = match dialog {
            Dialog::Message { .. } => (tr("Xabarlar"), 460.0),
            Dialog::Launch { .. } => (tr("Sayt yoki dasturni ochish"), 520.0),
            Dialog::File(_) => (tr("Fayl yuborish"), 500.0),
            Dialog::Apps(_) => (tr("Dasturlarni bloklash"), 480.0),
            Dialog::Net { .. } => (tr("Internet"), 540.0),
            Dialog::Power(_) => (tr("Kompyuterlarni boshqarish"), 540.0),
            Dialog::Settings(_) => (tr("Sozlamalar"), SETTINGS_W),
            Dialog::Room { rename: None, .. } => (tr("Yangi xona"), 440.0),
            Dialog::Room { .. } => (tr("Xona nomini o'zgartirish"), 440.0),
            Dialog::Activity => (tr("Faollik"), 440.0),
            Dialog::Freeze { .. } => (tr("Shadow rejim"), 520.0),
            Dialog::Rename { .. } => (tr("Nomini o'zgartirish"), 460.0),
            Dialog::Updates { .. } => (tr("Yangilanishlar"), 600.0),
        };

        // Drawers slide in from the right; modals fade in at the centre.
        // Drawers hang flush on the right edge of the window, full height; modals drop
        // from the top edge right under the navbar and tabs. Nothing floats free.
        let chrome_h = super::navbar::NAVBAR_H + super::navbar::TOOLBAR_H;
        let status_h = super::navbar::STATUS_H;
        let area = if drawer {
            let x = screen.right() - width + (1.0 - ease) * (width + 24.0);
            egui::Area::new(egui::Id::new("drawer"))
                .order(egui::Order::Foreground)
                .fixed_pos(pos2(x, screen.top()))
        } else {
            egui::Area::new(egui::Id::new("modal"))
                .order(egui::Order::Foreground)
                .anchor(Align2::CENTER_TOP, [0.0, chrome_h - (1.0 - ease) * 24.0])
        };

        let mut confirm = false;
        let mut cancel = false;
        let mut clear = false;
        let mut clear_help: Option<u64> = None;
        area.show(ctx, |ui| {
            ui.set_opacity(if drawer { ease.max(0.35) } else { ease });
            if !live {
                ui.disable();
            }
            let frame = Frame::none()
                .fill(p.card)
                .stroke(Stroke::new(1.0_f32, p.line))
                .rounding(Rounding::ZERO)
                .shadow(egui::epaint::Shadow {
                    offset: vec2(0.0, 16.0),
                    blur: 48.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }),
                });
            frame.show(ui, |ui| {
                ui.set_width(width);
                ui.set_max_width(width);
                // Header: one title, fixed height, close cell flush right and flush with the
                // bottom line (like the navbar). No item spacing here, so nothing leaves a gap.
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let header = ui.allocate_ui_with_layout(
                    vec2(width, HEADER_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new(title)
                                .family(ui_kit::bold())
                                .size(16.0)
                                .color(p.strong),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui_kit::icon_cell(ui, icons::X, HEADER_H, false)
                                .on_hover_text(tr("Yopish (Esc)"))
                                .clicked()
                            {
                                cancel = true;
                            }
                            ui_kit::rule(ui, HEADER_H);
                        });
                    },
                );
                let hr = header.response.rect;
                ui.painter().hline(
                    hr.x_range(),
                    hr.bottom() - 0.5,
                    Stroke::new(1.0_f32, p.line),
                );
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);

                // Body
                let has_footer = !matches!(
                    dialog,
                    Dialog::Activity
                        | Dialog::Message { .. }
                        | Dialog::Settings(_)
                        | Dialog::Freeze { .. }
                );
                let body_max = if drawer {
                    // Full window height: the drawer hangs on the right edge, top to bottom.
                    screen.height() - HEADER_H - 2.0 - if has_footer { FOOTER_H } else { 0.0 }
                } else {
                    (screen.height()
                        - chrome_h
                        - status_h
                        - HEADER_H
                        - 24.0
                        - if has_footer { FOOTER_H } else { 0.0 })
                    .max(160.0)
                };
                let body = |ui: &mut egui::Ui,
                            dialog: &mut Dialog,
                            confirm: &mut bool,
                            clear_help: &mut Option<u64>| {
                    Frame::none()
                        .inner_margin(Margin::symmetric(22.0, 18.0))
                        .show(ui, |ui| {
                            ui.set_width(width - 44.0);
                            if !matches!(
                                dialog,
                                Dialog::Settings(_)
                                    | Dialog::Freeze { .. }
                                    | Dialog::Room { .. }
                                    | Dialog::Activity
                                    | Dialog::Rename { .. }
                            ) {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(icons::USERS).color(p.faint));
                                    let shown =
                                        if matches!(dialog, Dialog::Power(PowerChoice::Wake)) {
                                            offline
                                        } else {
                                            n
                                        };
                                    ui_kit::hud(ui, &tf!("{n} ta kompyuterga", n = shown), p.muted);
                                });
                                ui.add_space(4.0);
                            }
                            dialog_body(
                                ui,
                                p,
                                dialog,
                                confirm,
                                clear_help,
                                &FormCtx {
                                    offline,
                                    offline_with_mac,
                                    recent: &recent,
                                    hub: &hub,
                                    all,
                                    targets: &targets,
                                },
                            );
                        });
                };
                if let Dialog::Settings(tab) = dialog {
                    // Edits go straight into the live config; the app reacts after the frame.
                    let mut c = cfg.write().unwrap();
                    let before = c.clone();
                    settings_page(ui, p, tab, &mut c, &scx, &mut settings_out);
                    if *c != before {
                        settings_before = Some(before);
                    }
                } else if let Dialog::Message { text, send } = dialog {
                    // The chat lays itself out: messages scroll, the composer stays at the bottom.
                    let size = vec2(width, body_max);
                    ui.allocate_ui(size, |ui| {
                        ui.set_min_size(size);
                        chat_form(
                            ui,
                            p,
                            text,
                            send,
                            &FormCtx {
                                offline,
                                offline_with_mac,
                                recent: &recent,
                                hub: &hub,
                                all,
                                targets: &targets,
                            },
                        );
                    });
                } else if drawer {
                    egui::ScrollArea::vertical()
                        .min_scrolled_height(body_max)
                        .max_height(body_max)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            body(ui, dialog, &mut confirm, &mut clear_help);
                        });
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(body_max)
                        .show(ui, |ui| body(ui, dialog, &mut confirm, &mut clear_help));
                }

                // Footer
                if has_footer {
                    let top = ui.cursor().top();
                    let footer_rect = Rect::from_min_max(
                        pos2(ui.min_rect().left(), top),
                        pos2(ui.min_rect().right(), top + FOOTER_H),
                    );
                    ui.painter().rect_filled(footer_rect, Rounding::ZERO, p.bg);
                    ui.painter()
                        .hline(footer_rect.x_range(), top, Stroke::new(1.0_f32, p.line));
                    Frame::none()
                        .inner_margin(Margin::symmetric(18.0, 14.0))
                        .show(ui, |ui| {
                            ui.set_width(width - 36.0);
                            ui.horizontal(|ui| {
                                if matches!(dialog, Dialog::Apps(_) | Dialog::Net { .. })
                                    && ui_kit::button(
                                        ui,
                                        Some(icons::TRASH_2),
                                        tr("Tozalash"),
                                        Kind::Outline,
                                    )
                                    .on_hover_text(tr("Barcha cheklovlarni olib tashlash"))
                                    .clicked()
                                {
                                    clear = true;
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let (pi, pl) = match dialog {
                                        Dialog::Message { .. } | Dialog::File(_) => {
                                            (icons::SEND, tr("Yuborish"))
                                        }
                                        Dialog::Launch { web: true, .. } => {
                                            (icons::GLOBE, tr("Ochish"))
                                        }
                                        Dialog::Launch { .. } => {
                                            (icons::PLAY, tr("Ishga tushirish"))
                                        }
                                        Dialog::Apps(_) => (icons::BAN, tr("Bloklash")),
                                        Dialog::Net { .. } => (icons::SHIELD_CHECK, tr("Qo'llash")),
                                        Dialog::Power(_) => (icons::POWER, tr("Tasdiqlash")),
                                        Dialog::Settings(_)
                                        | Dialog::Room { .. }
                                        | Dialog::Activity => (icons::CHECK, tr("Saqlash")),
                                        Dialog::Freeze { .. } => (icons::CHECK, tr("Saqlash")),
                                        Dialog::Rename { .. } => (icons::CHECK, tr("Saqlash")),
                                        Dialog::Updates { .. } => {
                                            (icons::CHECK, tr("Siyosatni saqlash"))
                                        }
                                    };
                                    let kind = if matches!(dialog, Dialog::Power(_)) {
                                        Kind::Danger
                                    } else {
                                        Kind::Primary
                                    };
                                    if ui_kit::button(ui, Some(pi), pl, kind).clicked() {
                                        confirm = true;
                                    }
                                    ui.add_space(6.0);
                                    if ui_kit::button(
                                        ui,
                                        Some(icons::X),
                                        tr("Bekor qilish"),
                                        Kind::Outline,
                                    )
                                    .clicked()
                                    {
                                        cancel = true;
                                    }
                                });
                            });
                        });
                }
            });
        });

        if let Some(before) = settings_before {
            self.settings_applied(ctx, before);
        }
        self.settings_actions(ctx, settings_out);
        if let Some(Dialog::Message { text, send }) = &mut self.dialog {
            self.hub.lock().unwrap().unread = 0;
            if std::mem::take(send) && !text.trim().is_empty() && !targets.is_empty() {
                let body = std::mem::take(text).trim().to_string();
                let who = recipients_label(all, &targets);
                self.send(
                    &targets,
                    Command::Message {
                        title: tr("O'qituvchidan xabar").into(),
                        text: body.clone(),
                    },
                    "",
                );
                let mut h = self.hub.lock().unwrap();
                h.chat_members.extend(targets.iter().copied());
                h.chat(true, who, body);
            }
        }
        self.shadow_requests(&targets);
        if let Some(Dialog::Updates {
            os,
            apps,
            restart,
            request,
            ..
        }) = &mut self.dialog
        {
            if let Some(r) = request.take() {
                let cmd = match r {
                    UpdateRequest::Check => PrivCommand::CheckUpdates,
                    UpdateRequest::Install => PrivCommand::InstallUpdates {
                        os: *os,
                        apps: *apps,
                        restart: *restart,
                    },
                };
                let what = if r == UpdateRequest::Check {
                    tr("Yangilanishlarni tekshirish")
                } else {
                    tr("Yangilanishlarni o'rnatish")
                };
                self.send_priv(&targets, cmd, what);
            }
        }
        if !live {
            ctx.request_repaint();
            return;
        }
        if backdrop_clicked || ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if let Some(id) = clear_help {
            self.apply(Action::ClearHelp(id), all);
        }
        if cancel {
            self.close_dialog();
        } else if confirm && matches!(self.dialog, Some(Dialog::Power(PowerChoice::Wake))) {
            self.wake(&wake_keys);
            self.close_dialog();
        } else if confirm || clear {
            let d = self.dialog.take().unwrap();
            self.closing = Some(d.clone());
            self.run_dialog(d, &targets, clear);
        }
    }

    /// Wake-on-LAN for the given (offline) computers.
    fn wake(&mut self, keys: &[String]) {
        let macs: Vec<[u8; 6]> = {
            let c = self.cfg.read().unwrap();
            keys.iter().filter_map(|k| wake_mac(&c, k)).collect()
        };
        let mut h = self.hub.lock().unwrap();
        if macs.is_empty() {
            h.toast(
                false,
                tr("Yoqish uchun manzil yo'q: kompyuter yangi agent bilan bir marta ulangan bo'lishi kerak."),
            );
            return;
        }
        let sent = macs
            .iter()
            .filter(|m| crate::net::wake_on_lan(**m).is_ok())
            .count();
        h.log(
            crate::net::Level::Info,
            tf!("Yoqish signali → {n} ta kompyuter", n = sent),
        );
        h.toast(
            sent > 0,
            if sent == 0 {
                tr("Yoqish signalini yuborib bo'lmadi (tarmoq?).").to_string()
            } else if sent < keys.len() {
                tf!(
                    "{n} ta kompyuterga yoqish signali yuborildi, {m} tasining manzili noma'lum.",
                    n = sent,
                    m = keys.len() - sent
                )
            } else {
                tf!("{n} ta kompyuterga yoqish signali yuborildi.", n = sent)
            },
        );
    }

    /// Inline actions of the Shadow drawer: they run at once and the drawer stays open.
    fn shadow_requests(&mut self, targets: &[u64]) {
        let Some(Dialog::Freeze {
            restart,
            folders,
            request,
            picked,
            password,
            ..
        }) = &mut self.dialog
        else {
            return;
        };
        let Some(r) = request.take() else {
            return;
        };
        let restart = *restart;
        let pw = password.clone();
        let folders = folders.clone();
        let paths: Vec<String> = if r == ShadowRequest::Commit {
            std::mem::take(picked).into_iter().collect()
        } else {
            vec![]
        };
        self.cfg.write().unwrap().shadow_password = pw.clone();
        self.send_priv(targets, PrivCommand::SetShadowPassword(pw), "");
        match r {
            ShadowRequest::Apply(action) => {
                let what = match action {
                    FreezeAction::Freeze => tr("Shadow yoqish"),
                    FreezeAction::Thaw => tr("Shadow o'chirish"),
                    FreezeAction::Maintenance => tr("Yangilab, qayta yoqish"),
                };
                self.send_priv(targets, PrivCommand::Freeze { action, restart }, what);
            }
            ShadowRequest::ListChanges => {
                self.send_priv(
                    targets,
                    PrivCommand::ListChanges,
                    tr("O'zgarishlar ro'yxati"),
                );
            }
            ShadowRequest::Commit => {
                self.send_priv(targets, PrivCommand::CommitPaths(paths), tr("Commit"));
            }
            ShadowRequest::SaveFolders => {
                self.cfg.write().unwrap().persistent_folders = folders.clone();
                self.send_priv(
                    targets,
                    PrivCommand::SetPersistentFolders(split_list(&folders)),
                    tr("Saqlanadigan papkalar"),
                );
            }
        }
    }

    pub(super) fn run_dialog(&mut self, d: Dialog, t: &[u64], clear: bool) {
        match d {
            Dialog::Message { text, .. } => {
                if !text.trim().is_empty() {
                    self.send(
                        t,
                        Command::Message {
                            title: tr("O'qituvchidan xabar").into(),
                            text,
                        },
                        tr("Xabar"),
                    );
                }
            }
            Dialog::Launch { web: true, url, .. } => {
                let Some(url) = normalize_url(&url) else {
                    return;
                };
                {
                    let mut c = self.cfg.write().unwrap();
                    c.recent_urls.retain(|u| *u != url);
                    c.recent_urls.insert(0, url.clone());
                    c.recent_urls.truncate(8);
                    crate::config::save(&c);
                }
                self.send(t, Command::OpenUrl(url), tr("Sayt ochish"))
            }
            Dialog::Launch { program, args, .. } => {
                let args = args.split_whitespace().map(String::from).collect();
                self.send(
                    t,
                    Command::Run {
                        program: program.trim().into(),
                        args,
                    },
                    tr("Dastur ishga tushirish"),
                );
            }
            Dialog::File(path) => {
                let path = path.trim().trim_matches('"').to_string();
                match std::fs::read(&path) {
                    Ok(data) if data.len() <= 100 * 1024 * 1024 => {
                        let name = std::path::Path::new(&path)
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.send(
                            t,
                            Command::SendFile {
                                name: name.clone(),
                                data,
                            },
                            &tf!("Fayl: {name}", name = name),
                        );
                    }
                    Ok(_) => self
                        .hub
                        .lock()
                        .unwrap()
                        .log(Level::Warn, tr("Fayl 100 MB dan katta")),
                    Err(e) => self
                        .hub
                        .lock()
                        .unwrap()
                        .log(Level::Warn, tf!("Faylni o'qib bo'lmadi: {e}", e = e)),
                }
            }
            Dialog::Apps(list) => {
                let items = if clear { vec![] } else { split_list(&list) };
                if !clear {
                    self.cfg.write().unwrap().app_blocklist = list;
                }
                self.send(
                    t,
                    Command::SetAppBlocklist(items),
                    if clear {
                        tr("Dastur bloki olib tashlandi")
                    } else {
                        tr("Dasturlar bloklandi")
                    },
                );
            }
            Dialog::Net {
                domains,
                keywords,
                safe,
                ..
            } => {
                let policy = if clear {
                    NetPolicy::default()
                } else {
                    let mut c = self.cfg.write().unwrap();
                    c.web_blocklist = domains.clone();
                    c.keyword_blocklist = keywords.clone();
                    c.safe_search = safe;
                    NetPolicy {
                        domains: split_list(&domains),
                        keywords: split_list(&keywords),
                        safe_search: safe,
                    }
                };
                let what = if clear {
                    tr("NetControl o'chirildi").to_string()
                } else {
                    tf!(
                        "NetControl: {v0} domen, {v1} kalit so'z{v2}",
                        v0 = policy.domains.len(),
                        v1 = policy.keywords.len(),
                        v2 = if safe { ", SafeSearch" } else { "" }
                    )
                };
                self.send_priv(t, PrivCommand::SetNetPolicy(policy), &what);
            }
            Dialog::Power(a) => {
                let (act, what) = match a {
                    PowerChoice::Shutdown => (PowerAction::Shutdown, tr("O'chirish")),
                    PowerChoice::Restart => (PowerAction::Restart, tr("Qayta yuklash")),
                    PowerChoice::Logoff => (PowerAction::Logoff, tr("Seansdan chiqarish")),
                    PowerChoice::Wake => return, // handled before (offline computers)
                };
                self.send_priv(t, PrivCommand::Power(act), what);
            }
            Dialog::Room { name, rename } => {
                self.save_room(name, rename);
                return;
            }
            Dialog::Activity => return,
            Dialog::Freeze { .. } => return,
            Dialog::Rename {
                id,
                key,
                alias,
                system,
                system_name,
                restart,
            } => {
                let alias = alias.trim().to_string();
                {
                    let mut c = self.cfg.write().unwrap();
                    if alias.is_empty() {
                        c.aliases.remove(&key);
                    } else {
                        c.aliases.insert(key.clone(), alias);
                    }
                }
                let new = system_name.trim().to_string();
                let current = all_hostname(&self.hub, id);
                if system && Some(&new) != current.as_ref() {
                    if !vimora_core::valid_computer_name(&new) {
                        self.hub.lock().unwrap().log(
                            Level::Warn,
                            tf!("Noto'g'ri kompyuter nomi: {new}", new = new),
                        );
                    } else {
                        // Rooms and aliases are keyed by hardware id, so they survive the rename.
                        self.send_priv(
                            &[id],
                            PrivCommand::RenameComputer { name: new, restart },
                            tr("Kompyuter nomi"),
                        );
                    }
                }
            }
            Dialog::Updates { policy, .. } => {
                self.cfg.write().unwrap().update_policy = policy;
                self.send_priv(
                    t,
                    PrivCommand::SetUpdatePolicy(policy),
                    tr("Yangilanish siyosati"),
                );
            }
            Dialog::Settings(_) => {}
        }
        config::save(&self.cfg.read().unwrap());
    }

    /// Side effects of a live settings change (the config itself is already updated).
    fn settings_applied(&mut self, ctx: &egui::Context, before: ServerConfig) {
        let mut c = self.cfg.write().unwrap();
        if c.language != before.language {
            vimora_ui::i18n::set_lang(c.language);
            // Untouched lock-screen texts follow the language.
            let (old_t, old_m) = config::default_lock(before.language);
            let (new_t, new_m) = config::default_lock(c.language);
            if c.lock_title == old_t {
                c.lock_title = new_t.into();
            }
            if c.lock_message == old_m {
                c.lock_message = new_m.into();
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                vimora_ui::tr("Vimora — O'qituvchi konsoli").into(),
            ));
        }
        c.grid_columns = c.grid_columns.clamp(MIN_COLUMNS, MAX_COLUMNS);
        let stream = (c.thumb_interval_ms != before.thumb_interval_ms).then(|| thumb_stream(&c));
        drop(c);
        if let Some(s) = stream {
            let ids: Vec<u64> = self.hub.lock().unwrap().students.keys().copied().collect();
            self.send(&ids, Command::SetStream(s), "");
        }
        self.settings_dirty = Some(Instant::now());
    }

    fn settings_actions(&mut self, ctx: &egui::Context, out: SettingsOut) {
        if out.check_update {
            self.updater.check();
        }
        if out.install_update {
            self.updater.install();
        }
        if out.restart {
            config::save(&self.cfg.read().unwrap());
            crate::update::relaunch();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let everyone: Vec<u64> = self.hub.lock().unwrap().students.keys().copied().collect();
        if out.push_policy {
            let policy = self.cfg.read().unwrap().update_policy;
            self.send_priv(
                &everyone,
                PrivCommand::SetUpdatePolicy(policy),
                tr("Yangilanish siyosati"),
            );
        }
        if out.update_students {
            self.send_priv(
                &everyone,
                PrivCommand::UpdateVimora,
                tr("Vimora yangilanishi"),
            );
        }
    }
}

/// The address to wake a computer at: what it reported when it last connected, else the
/// hardware id the console already uses as its key.
fn wake_mac(c: &crate::config::ServerConfig, key: &str) -> Option<[u8; 6]> {
    c.known_pcs
        .get(key)
        .and_then(|k| crate::net::parse_mac(&k.mac))
        .or_else(|| crate::net::parse_mac(key.split('/').next().unwrap_or("")))
}

/// What the teacher typed → a URL the students' browsers can open: adds `https://`, and turns
/// plain words into a web search. `None` for an empty box.
pub(super) fn normalize_url(input: &str) -> Option<String> {
    let t = input.trim().trim_matches('"').trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    if lower.contains("://") || lower.starts_with("mailto:") {
        return Some(t.to_string());
    }
    let looks_like_site = !t.contains(' ')
        && (t.contains('.') || lower.starts_with("localhost"))
        && !t.ends_with('.');
    if looks_like_site {
        return Some(format!("https://{t}"));
    }
    let q: String = t
        .chars()
        .map(|c| match c {
            ' ' => "+".to_string(),
            c if c.is_ascii_alphanumeric() || "-_.~".contains(c) => c.to_string(),
            c => c
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect::<String>(),
        })
        .collect();
    Some(format!("https://www.google.com/search?q={q}"))
}

#[cfg(test)]
mod url_tests {
    use super::normalize_url;

    #[test]
    fn urls() {
        assert_eq!(normalize_url("  "), None);
        assert_eq!(normalize_url("youtube.com").unwrap(), "https://youtube.com");
        assert_eq!(normalize_url("http://a.uz/x").unwrap(), "http://a.uz/x");
        assert_eq!(
            normalize_url("o'zbek tili").unwrap(),
            "https://www.google.com/search?q=o%27zbek+tili"
        );
        assert_eq!(
            normalize_url("localhost:8080").unwrap(),
            "https://localhost:8080"
        );
    }
}
