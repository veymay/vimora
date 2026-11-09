#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Vimora teacher console.

mod app;
mod config;
mod i18n_table;
mod net;
mod sound;
mod update;

use std::sync::{Arc, Mutex, RwLock};

/// The window has exactly this size, unless it is maximized or fullscreen.
pub const MIN_WINDOW: [f32; 2] = [1280.0, 760.0];

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn,eframe=warn".into()),
        )
        .init();

    let cfg = config::load();
    vimora_ui::i18n::install(i18n_table::ROWS);
    vimora_ui::i18n::set_lang(cfg.language);
    let maximized = cfg.start_maximized;
    let check_updates = cfg.check_updates;
    let cfg = Arc::new(RwLock::new(cfg));
    let hub: net::SharedHub = Arc::new(Mutex::new(net::Hub::new(config::identity())));
    let show = Arc::new(net::Show::default());
    net::start(hub.clone(), cfg.clone(), show.clone());
    let updater = update::Updater::new();
    if check_updates {
        updater.start_background_checks();
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(vimora_ui::tr("Vimora — O'qituvchi konsoli"))
        .with_app_id("vimora-server")
        .with_inner_size(MIN_WINDOW)
        .with_min_inner_size(MIN_WINDOW)
        .with_maximized(maximized);
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/brand/vimora-256.png"))
    {
        viewport = viewport.with_icon(icon);
    }
    let opts = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "vimora-server",
        opts,
        Box::new(move |cc| {
            hub.lock().unwrap().ctx = Some(cc.egui_ctx.clone());
            Ok(Box::new(app::ConsoleApp::new(cc, hub, cfg, show, updater)))
        }),
    )
}
