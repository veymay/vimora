#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Vimora teacher console.

mod app;
mod config;
mod i18n_table;
mod net;
mod single;
mod sound;
mod update;
#[cfg(all(unix, not(target_os = "macos")))]
mod wayland_cast;

use std::sync::{Arc, Mutex, RwLock};

/// The window has exactly this size and can't be resized; only fullscreen (F11) changes it.
pub const MIN_WINDOW: [f32; 2] = [1120.0, 700.0];

fn main() -> eframe::Result<()> {
    // Ubuntu Wayland: a native Wayland window can't be un-minimized or focused by the app, so the
    // tray's "Open" (and a second start) couldn't bring the console back. Under XWayland it can.
    // Screen capture for presenting goes through the desktop portal either way.
    #[cfg(all(unix, not(target_os = "macos")))]
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_some()
        && std::env::var_os("VIMORA_WAYLAND").is_none()
    {
        // Still single-threaded here, so changing the environment is safe.
        std::env::remove_var("WAYLAND_DISPLAY");
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn,eframe=warn".into()),
        )
        .init();

    // A second start just brings the running console to the front.
    let Some(instance) = single::claim() else {
        return Ok(());
    };
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
        .with_resizable(false)
        .with_maximize_button(false)
        .with_fullscreen(maximized);
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
            single::serve(instance, cc.egui_ctx.clone());
            Ok(Box::new(app::ConsoleApp::new(cc, hub, cfg, show, updater)))
        }),
    )
}
