//! Shared parts of Vimora's small windows (the setup window, the tray window): no system title
//! bar — our own strip with the logo, a title and only an X; dragging the strip moves the window.

use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Sense, Stroke};
use vimora_ui::icons;

pub const TITLE_H: f32 = 40.0;

/// Window options for a small fixed-size window without a system title bar.
pub fn options(app_id: &str, size: [f32; 2], position: Option<[f32; 2]>) -> eframe::NativeOptions {
    let mut vp = egui::ViewportBuilder::default()
        .with_title("Vimora")
        .with_app_id(app_id)
        .with_inner_size(size)
        .with_resizable(false)
        .with_decorations(false)
        .with_minimize_button(false)
        .with_maximize_button(false);
    if let Some(p) = position {
        vp = vp.with_position(p).with_always_on_top();
    }
    eframe::NativeOptions {
        viewport: vp,
        centered: position.is_none(),
        renderer: crate::ui_renderer(),
        ..Default::default()
    }
}

/// Fonts and theme (follows the system's light / dark choice).
pub fn style(ctx: &egui::Context) {
    vimora_ui::install_fonts(ctx);
    let mode = if ctx.style().visuals.dark_mode {
        vimora_ui::ThemeMode::Dark
    } else {
        vimora_ui::ThemeMode::Light
    };
    vimora_ui::apply_theme(ctx, mode);
}

/// The title strip. Returns true when X was pressed.
pub fn title_bar(ctx: &egui::Context, title: &str) -> bool {
    let p = vimora_ui::palette(ctx);
    let mut close = false;
    egui::TopBottomPanel::top("title")
        .exact_height(TITLE_H)
        .frame(egui::Frame::none().fill(p.panel))
        .show(ctx, |ui| {
            let r = ui.max_rect();
            let drag = ui.interact(r, egui::Id::new("title-drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.painter()
                .hline(r.x_range(), r.bottom(), Stroke::new(1.0_f32, p.line));
            let logo =
                Rect::from_min_size(pos2(r.left() + 12.0, r.center().y - 10.0), vec2(20.0, 20.0));
            vimora_ui::logo(ui.painter(), logo);
            ui.painter().text(
                pos2(logo.right() + 9.0, r.center().y),
                Align2::LEFT_CENTER,
                title,
                FontId::new(14.0, vimora_ui::bold()),
                p.strong,
            );
            let x = Rect::from_min_max(pos2(r.right() - TITLE_H, r.top()), r.max);
            let resp = ui.interact(x, egui::Id::new("title-x"), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(x, 0.0, p.danger);
            }
            ui.painter().text(
                x.center(),
                Align2::CENTER_CENTER,
                icons::X,
                FontId::proportional(16.0),
                if resp.hovered() { p.invert_fg } else { p.muted },
            );
            close = resp.clicked();
        });
    close
}

/// Dev aid: `VIMORA_SCREENSHOT=<file>` saves the window after `VIMORA_SCREENSHOT_DELAY` seconds
/// (default 2) and exits.
pub fn screenshot(ctx: &egui::Context) {
    let Some(path) = std::env::var_os("VIMORA_SCREENSHOT") else {
        return;
    };
    let delay: f64 = std::env::var("VIMORA_SCREENSHOT_DELAY")
        .ok()
        .and_then(|d| d.parse().ok())
        .unwrap_or(2.0);
    let shot = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(img) = shot {
        let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba)
        {
            let _ = image::DynamicImage::ImageRgba8(buf).to_rgb8().save(&path);
        }
        std::process::exit(0);
    }
    if ctx.input(|i| i.time) > delay {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}
