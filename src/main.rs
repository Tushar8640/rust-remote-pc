mod common;
mod host;
mod client;
mod ui;

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "rust-remote-pc")]
#[command(author = "Imran")]
#[command(version = "0.1.0")]
#[command(about = "High-performance, low-latency Rust remote desktop over Tailscale", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run as Headless Host Agent in terminal
    Host {
        /// Bind address (e.g. 0.0.0.0:5885 or specific Tailscale IP:port)
        #[arg(short, long, default_value = "0.0.0.0:5885")]
        bind: String,

        /// Pre-shared authentication password/token
        #[arg(short, long, default_value = "secret123")]
        password: String,

        /// Target streaming frame rate (FPS)
        #[arg(long, default_value_t = 30)]
        fps: u32,

        /// JPEG compression quality (1-100)
        #[arg(short, long, default_value_t = 75)]
        quality: u8,
    },

    /// Run as Client Viewer GUI connecting directly
    Client {
        /// Host Tailscale address (e.g. 100.x.y.z:5885)
        #[arg(short = 'H', long)]
        host: Option<String>,

        /// Pre-shared authentication password/token
        #[arg(short, long)]
        password: Option<String>,
    },
}

fn main() -> Result<(), anyhow::Error> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Create Tokio Multi-threaded Runtime
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    );

    match cli.command {
        Some(Commands::Host {
            bind,
            password,
            fps,
            quality,
        }) => {
            info!("Starting Rust Remote Desktop - Headless Host Agent");
            let config = host::server::HostServerConfig {
                bind_addr: bind,
                password,
                target_fps: fps,
                jpeg_quality: quality,
            };

            let server = host::server::HostServer::new(config);
            rt.block_on(server.run())?;
        }
        Some(Commands::Client { host, password }) => {
            info!("Starting Rust Remote Desktop - Client Mode");
            launch_unified_gui(host, password, rt)?;
        }
        None => {
            info!("No CLI command provided. Launching full Unified Desktop GUI!");
            launch_unified_gui(None, None, rt)?;
        }
    }

    Ok(())
}

fn launch_unified_gui(
    host: Option<String>,
    password: Option<String>,
    rt: Arc<tokio::runtime::Runtime>,
) -> Result<(), anyhow::Error> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("Remote Desktop"),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Remote Desktop",
        native_options,
        Box::new(|cc| {
            Ok(Box::new(ui::UnifiedRemoteApp::new(
                cc, host, password, rt,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI runtime error: {:?}", e))?;

    Ok(())
}
