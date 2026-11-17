// Release builds on Windows run without a teacher window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Vimora student agent.
//!
//! The agent itself is headless. It discovers the teacher on the LAN, authenticates
//! with the classroom key and executes commands. Fullscreen UI (lock screen, teacher-screen
//! presentation) and popup messages run as short-lived child processes of the same binary
//! (`vimora-student overlay` / `vimora-student popup`) which the agent supervises.

mod about;
mod agent;
mod apps;
mod audio;
mod blocked;
mod chat;
mod clip;
mod config;
mod desktop;
mod devices;
mod firewall;
mod guard;
mod inputlock;
mod kiosk;
mod layout;
mod lessonui;
mod netcontrol;
mod overlay;
mod popup;
mod power;
mod privileged;
mod purge;
mod secinfo;
mod selfcheck;
mod service;
mod sh;
mod shield;
mod single;
mod smallwin;
mod sysmon;
mod system;
mod updates;
mod watched;
mod wizard;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vimora-student", version, about = "VIMORA student agent")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Connect directly to this teacher (ip:port) instead of LAN discovery.
    #[arg(long, global = true)]
    server: Option<String>,
    /// Display name shown in the teacher.
    #[arg(long, global = true)]
    name: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the agent (default).
    Run {
        /// Internal: the service started it again after it was ended.
        #[arg(long, hide = true)]
        restarted: bool,
    },
    /// Write the effective configuration to the config file and exit.
    Init,
    /// (Admin) Install: write configs, register and start the service.
    Setup,
    /// (Admin) Forget the paired teacher; the next teacher found is paired.
    Unpair,
    /// Privileged Vimora service (runs as SYSTEM / root).
    Service,
    /// Internal (Windows): watches the service and starts it again within a second if it is
    /// ended; the service does the same for this watcher.
    #[command(hide = true)]
    Guard,
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
    /// Vimora's own setup window (started by the Windows installer).
    #[command(hide = true)]
    SetupWizard { installer: String },
    /// Exit code 0 when this computer is frozen right now (UWF / Shadow Defender), 1 when not.
    #[command(hide = true)]
    IsFrozen,
    /// The small window of the tray icon.
    #[command(hide = true)]
    About,
    /// (Admin) Check GitHub for a newer signed Vimora and install it (the scheduled task).
    #[command(hide = true)]
    UpdateNow,
    /// (Admin) Used by the uninstaller: removes everything Vimora left on this computer.
    #[command(hide = true)]
    Purge,
}

/// The service and its watcher have no console: they write `service.log` next to the service's
/// settings (kept small), so a failure leaves a trace to read.
fn log_file() -> Option<std::fs::File> {
    let dir = service::config_path().parent()?.to_path_buf();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("service.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 512 * 1024) {
        let _ = std::fs::rename(&path, dir.join("service.old.log"));
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,wgpu=warn,eframe=warn".into());
    let background = matches!(
        std::env::args().nth(1).as_deref(),
        Some("service" | "guard")
    );
    match background.then(log_file).flatten() {
        Some(f) => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(f))
            .init(),
        None => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init(),
    }
    std::panic::set_hook(Box::new(|info| tracing::error!("panic: {info}")));

    let cli = Cli::parse();
    // The agent, its windows (lock screen, chat, …) and the service can't be ended by the
    // student from Task Manager (the service runs as SYSTEM and keeps every right).
    if !matches!(
        cli.cmd,
        Some(
            Cmd::Setup
                | Cmd::Unpair
                | Cmd::Init
                | Cmd::Purge
                | Cmd::UpdateNow
                | Cmd::UiProbe
                | Cmd::SetupWizard { .. }
                | Cmd::IsFrozen
        )
    ) {
        // A computer under the write filter (UWF) is left exactly as it was before the extra
        // hardening: the service and the watcher are not locked down there.
        let hands_off = matches!(cli.cmd, Some(Cmd::Service | Cmd::Guard))
            && shield::Shield::new().protected_now();
        if hands_off {
            tracing::info!("write filter is on: the service keeps its default permissions");
        } else {
            guard::protect_self();
        }
    }
    match cli.cmd {
        Some(Cmd::Overlay) => overlay::run(),
        Some(Cmd::Popup { title, text, reply }) => popup::run(&title, &text, reply),
        Some(Cmd::Blocked { word }) => blocked::run(&word),
        Some(Cmd::Watched) => watched::run(),
        Some(Cmd::Poll) => lessonui::run_poll(),
        Some(Cmd::Banner { goal, end_unix }) => lessonui::run_banner(goal, end_unix),
        Some(Cmd::Test) => lessonui::run_test(),
        Some(Cmd::Service) => service::run(),
        Some(Cmd::Guard) => guard::run_guard(),
        Some(Cmd::Chat) => chat::run(),
        Some(Cmd::UiProbe) => ui_probe(),
        Some(Cmd::Setup) => service::setup(cli.server, cli.name),
        Some(Cmd::Unpair) => service::unpair(),
        Some(Cmd::Purge) => purge::run(),
        Some(Cmd::UpdateNow) => privileged::update_now(),
        Some(Cmd::SetupWizard { installer }) => wizard::run(installer),
        Some(Cmd::IsFrozen) => std::process::exit(if shield::Shield::new().protected_now() {
            0
        } else {
            1
        }),
        Some(Cmd::About) => about::run(),
        Some(Cmd::Init) => {
            let cfg = config::load().merge_cli(cli.server, cli.name);
            let path = config::save(&cfg)?;
            println!("config written to {}", path.display());
            Ok(())
        }
        Some(Cmd::Run { restarted }) => run_agent(cli.server, cli.name, restarted),
        None => run_agent(cli.server, cli.name, false),
    }
}

fn run_agent(server: Option<String>, name: Option<String>, restarted: bool) -> anyhow::Result<()> {
    {
        {
            if restarted {
                agent::TAMPERED.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            let cfg = config::load().merge_cli(server, name);
            single::take_over();
            single::watch();
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

/// `vimora-student ui-probe`: opens a hidden window, draws two frames, exits 0. If the renderer
/// cannot start, the process ends with an error instead and the agent tries the other one.
fn ui_probe() -> anyhow::Result<()> {
    struct Probe(u32);
    impl eframe::App for Probe {
        fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("VIMORA");
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
            .with_title("VIMORA")
            .with_icon(crate::smallwin::icon())
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
