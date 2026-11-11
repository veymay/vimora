// Release builds on Windows run without a console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Vimora student agent.
//!
//! The agent itself is headless. It discovers the teacher console on the LAN, authenticates
//! with the classroom key and executes commands. Fullscreen UI (lock screen, teacher-screen
//! presentation) and popup messages run as short-lived child processes of the same binary
//! (`vimora-client overlay` / `vimora-client popup`) which the agent supervises.

mod agent;
mod apps;
mod audio;
mod blocked;
mod chat;
mod config;
mod desktop;
mod devices;
mod firewall;
mod inputlock;
mod kiosk;
mod lessonui;
mod netcontrol;
mod overlay;
mod popup;
mod power;
mod privileged;
mod secinfo;
mod selfcheck;
mod service;
mod sh;
mod shield;
mod single;
mod sysmon;
mod system;
mod updates;
mod watched;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vimora-client", version, about = "Vimora student agent")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Connect directly to this console (ip:port) instead of LAN discovery.
    #[arg(long, global = true)]
    server: Option<String>,
    /// Display name shown in the teacher console.
    #[arg(long, global = true)]
    name: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the agent (default).
    Run,
    /// Write the effective configuration to the config file and exit.
    Init,
    /// (Admin) Install: write configs, register and start the service.
    Setup,
    /// (Admin) Forget the paired console; the next console found is paired.
    Unpair,
    /// Privileged Vimora service (runs as SYSTEM / root).
    Service,
    /// Internal: fullscreen lock / presentation overlay (driven over stdin).
    #[command(hide = true)]
    Overlay,
    /// Internal: checks that a window can be drawn with the chosen renderer, then exits 0.
    #[command(hide = true)]
    UiProbe,
    /// Internal: docked chat panel (driven over stdin).
    #[command(hide = true)]
    Chat,
    /// Internal: popup message window.
    #[command(hide = true)]
    Popup {
        title: String,
        text: String,
        /// Show a reply box; the reply is printed to stdout as one JSON line.
        #[arg(long)]
        reply: bool,
    },
    /// Internal: full-screen "forbidden search" modal.
    #[command(hide = true)]
    Blocked { word: String },
    /// Internal: "the teacher is watching" pill at the top of the screen.
    #[command(hide = true)]
    Watched,
    /// Internal: quick question window (JSON on stdin).
    #[command(hide = true)]
    Poll,
    /// Internal: lesson goal / countdown at the top of the screen.
    #[command(hide = true)]
    Banner { goal: String, end_unix: u64 },
    /// Internal: fullscreen test (JSON on stdin).
    #[command(hide = true)]
    Test,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn,eframe=warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Overlay) => overlay::run(),
        Some(Cmd::Popup { title, text, reply }) => popup::run(&title, &text, reply),
        Some(Cmd::Blocked { word }) => blocked::run(&word),
        Some(Cmd::Watched) => watched::run(),
        Some(Cmd::Poll) => lessonui::run_poll(),
        Some(Cmd::Banner { goal, end_unix }) => lessonui::run_banner(goal, end_unix),
        Some(Cmd::Test) => lessonui::run_test(),
        Some(Cmd::Service) => service::run(),
        Some(Cmd::Chat) => chat::run(),
        Some(Cmd::UiProbe) => ui_probe(),
        Some(Cmd::Setup) => service::setup(cli.server, cli.name),
        Some(Cmd::Unpair) => service::unpair(),
        Some(Cmd::Init) => {
            let cfg = config::load().merge_cli(cli.server, cli.name);
            let path = config::save(&cfg)?;
            println!("config written to {}", path.display());
            Ok(())
        }
        Some(Cmd::Run) | None => {
            let cfg = config::load().merge_cli(cli.server, cli.name);
            single::take_over();
            system::set_dpi_aware();
            std::panic::set_hook(Box::new(|info| tracing::error!("panic: {info}")));
            watchdog();
            // The agent must never stay down: a panic or an error restarts it in-process.
            loop {
                let c = cfg.clone();
                let res = std::panic::catch_unwind(move || -> anyhow::Result<()> {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()?;
                    rt.block_on(agent::run(c))
                });
                match res {
                    Ok(Ok(())) => tracing::warn!("agent returned; restarting"),
                    Ok(Err(e)) => tracing::error!("agent failed: {e:#}; restarting"),
                    Err(_) => tracing::error!("agent panicked; restarting"),
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }
}

/// If the agent shows no sign of life for two minutes (a frozen runtime, a hung call), start a
/// fresh copy of the program and leave. The new copy ends this one anyway (`single.rs`).
fn watchdog() {
    agent::HEARTBEAT.store(agent::now_secs(), std::sync::atomic::Ordering::Relaxed);
    let _ = std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(20));
            let last = agent::HEARTBEAT.load(std::sync::atomic::Ordering::Relaxed);
            // After a sleep/resume the clock jumps; only a stall that persists counts.
            if agent::now_secs().saturating_sub(last) > 120 {
                tracing::error!("agent stalled — restarting");
                std::thread::sleep(std::time::Duration::from_secs(10));
                let last = agent::HEARTBEAT.load(std::sync::atomic::Ordering::Relaxed);
                if agent::now_secs().saturating_sub(last) > 120 {
                    let args: Vec<String> = std::env::args().skip(1).collect();
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(exe).args(args).spawn();
                    }
                    std::process::exit(1);
                }
            }
        });
}

/// Which graphics backend the windows (lock screen, chat, popups) use. The agent picks one
/// that works on this PC (see `agent::probe_ui`) and tells its children through
/// `VIMORA_RENDERER`: OpenGL ("glow") needs a real graphics driver, Direct3D / Vulkan ("wgpu")
/// also runs on PCs with the basic display driver.
pub fn ui_renderer() -> eframe::Renderer {
    match std::env::var("VIMORA_RENDERER").as_deref() {
        Ok("wgpu") => eframe::Renderer::Wgpu,
        _ => eframe::Renderer::Glow,
    }
}

/// `vimora-client ui-probe`: opens a hidden window, draws two frames, exits 0. If the renderer
/// cannot start, the process ends with an error instead and the agent tries the other one.
fn ui_probe() -> anyhow::Result<()> {
    struct Probe(u32);
    impl eframe::App for Probe {
        fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Vimora");
            });
            self.0 += 1;
            if self.0 >= 2 {
                std::process::exit(0);
            }
            ctx.request_repaint();
        }
    }
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Vimora")
            .with_inner_size([160.0, 90.0])
            .with_decorations(false)
            .with_taskbar(false)
            .with_visible(false),
        renderer: ui_renderer(),
        ..Default::default()
    };
    eframe::run_native("vimora-probe", opts, Box::new(|_| Ok(Box::new(Probe(0)))))
        .map_err(|e| anyhow::anyhow!("ui probe: {e}"))?;
    anyhow::bail!("ui probe: window closed before drawing")
}
