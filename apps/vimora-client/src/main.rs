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
mod chat;
mod config;
mod netcontrol;
mod overlay;
mod popup;
mod privileged;
mod service;
mod sh;
mod shield;
mod system;
mod updates;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vimora-client", version, about = "Vimora student agent")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Classroom key (overrides the config file).
    #[arg(long, global = true)]
    key: Option<String>,
    /// Room name to join (overrides the config file). Empty = any room.
    #[arg(long, global = true)]
    room: Option<String>,
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
    /// (Admin) Install mode: key → protected service config, register and start the service.
    Setup,
    /// Privileged Vimora service (runs as SYSTEM / root).
    Service,
    /// Internal: fullscreen lock / presentation overlay (driven over stdin).
    #[command(hide = true)]
    Overlay,
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
        Some(Cmd::Service) => service::run(),
        Some(Cmd::Chat) => chat::run(),
        Some(Cmd::Setup) => service::setup(cli.key.as_deref().unwrap_or(""), cli.server, cli.name),
        Some(Cmd::Init) => {
            let cfg = config::load().merge_cli(cli.key, cli.room, cli.server, cli.name);
            let path = config::save(&cfg)?;
            println!("config written to {}", path.display());
            Ok(())
        }
        Some(Cmd::Run) | None => {
            let cfg = config::load().merge_cli(cli.key, cli.room, cli.server, cli.name);
            system::set_dpi_aware();
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(agent::run(cfg))
        }
    }
}
