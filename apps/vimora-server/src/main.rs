#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Vimora teacher console.

mod app;
mod config;
mod net;

use std::sync::{Arc, Mutex, RwLock};

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn,eframe=warn".into()),
        )
        .init();

    let cfg = Arc::new(RwLock::new(config::load()));
    let hub: net::SharedHub = Arc::new(Mutex::new(net::Hub::default()));
    let show = Arc::new(net::Show::default());
    net::start(hub.clone(), cfg.clone(), show.clone());

    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Vimora — O'qituvchi konsoli")
            .with_inner_size([1480.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-server",
        opts,
        Box::new(move |cc| {
            hub.lock().unwrap().ctx = Some(cc.egui_ctx.clone());
            Ok(Box::new(app::ConsoleApp::new(cc, hub, cfg, show)))
        }),
    )
}
