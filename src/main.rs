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
#[command(about = "High-performance, low-latency Rust remote desktop over P2P, LAN or Tailscale", long_about = None)]
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

        /// Do not stream this PC's audio to clients
        #[arg(long)]
        no_audio: bool,

        /// Disable peer-to-peer (internet) connections; accept direct TCP only
        #[arg(long)]
        no_p2p: bool,

        /// Video quality mode offered to viewers: auto, smooth, sharp or saver
        #[arg(long)]
        quality_mode: Option<String>,

        /// Use the premium relay (needs the premium URL and licence from the saved settings
        /// or RRPC_PREMIUM_URL / RRPC_PREMIUM_LICENSE)
        #[arg(long)]
        premium: bool,
    },

    /// Connect headlessly, wait for one video frame and report (connectivity self-test)
    Check {
        /// Host address (host:port) or 64-character Pairing ID
        #[arg(short = 'H', long)]
        host: String,

        /// Host password
        #[arg(short, long)]
        password: String,

        /// Quality mode to request: auto, smooth, sharp or saver
        #[arg(long)]
        quality_mode: Option<String>,
    },

    /// Test that the premium relay is reachable and accepts this PC's licence
    RelayCheck {
        /// Relay URL (defaults to the saved setting or RRPC_PREMIUM_URL)
        #[arg(long)]
        url: Option<String>,
    },

    /// Print the join code to share so clients can connect through the premium relay
    JoinCode,

    /// Run as Client Viewer GUI connecting directly
    Client {
        /// Host address (e.g. 100.x.y.z:5885) or 64-character Pairing ID
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
            no_audio,
            no_p2p,
            quality_mode,
            premium,
        }) => {
            info!("Starting Rust Remote Desktop - Headless Host Agent");
            let mut settings = common::transport::load_settings();
            if premium {
                settings.premium_enabled = true;
            }
            let premium_relay = settings.host_relay().map_err(|e| anyhow::anyhow!(e))?;
            let p2p_key = if no_p2p {
                None
            } else {
                let key = common::transport::load_or_create_secret_key();
                println!(
                    "Pairing ID (connect from anywhere): {}",
                    common::transport::pairing_id(&key)
                );
                if let Some(code) = settings.join_code(&common::transport::pairing_id(&key)) {
                    println!("Join code (share with premium clients, valid about a day): {code}");
                }
                Some(key)
            };
            let config = host::server::HostServerConfig {
                bind_addr: bind,
                password,
                target_fps: fps,
                jpeg_quality: quality,
                audio_enabled: !no_audio,
                p2p_key,
                quality_mode: parse_mode(quality_mode, settings.host_quality)?,
                premium_relay,
            };

            let server = host::server::HostServer::new(config);
            rt.block_on(server.run())?;
        }
        Some(Commands::Check { host, password, quality_mode }) => {
            let opts = client::connection::ConnectOptions {
                premium: None,
                quality: parse_mode(quality_mode, common::protocol::QualityMode::Auto)?,
            };
            rt.block_on(async {
                let started = std::time::Instant::now();
                let (tx, mut rx) = tokio::sync::mpsc::channel(4);
                let handle = client::connection::connect_to_host(&host, &password, tx, opts).await?;
                println!(
                    "connected in {} ms, remote screen {}x{}",
                    started.elapsed().as_millis(),
                    handle.screen_width,
                    handle.screen_height
                );
                // Measure for 10 s. Frame latency = now - host capture time; only
                // meaningful when host and client share a clock (same machine).
                let (mut n, mut lat_sum, mut lat_max) = (0u64, 0u64, 0u64);
                let t0 = std::time::Instant::now();
                while t0.elapsed() < std::time::Duration::from_secs(10) {
                    match tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv()).await {
                        Ok(Some(f)) => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            let lat = now.saturating_sub(f.timestamp_ms);
                            n += 1;
                            lat_sum += lat;
                            lat_max = lat_max.max(lat);
                        }
                        _ => return Err(anyhow::anyhow!("no video frame within 15 s")),
                    }
                }
                println!(
                    "{} frames in 10 s ({:.1} fps) | frame latency avg {} ms, max {} ms | rtt {} ms",
                    n,
                    n as f64 / 10.0,
                    lat_sum / n.max(1),
                    lat_max,
                    handle.rtt_ms.load(std::sync::atomic::Ordering::Relaxed)
                );
                println!("link: {}", handle.link.lock().map(|l| l.clone()).unwrap_or_default());
                Ok::<(), anyhow::Error>(())
            })?;
        }
        Some(Commands::RelayCheck { url }) => {
            let mut settings = common::transport::load_settings();
            settings.premium_enabled = true;
            if let Some(u) = url {
                settings.premium_url = u;
            }
            let p = settings
                .host_relay()
                .map_err(|e| anyhow::anyhow!(e))?
                .ok_or_else(|| anyhow::anyhow!("premium is not configured"))?;
            match rt.block_on(common::transport::test_premium_relay(&p)) {
                Ok(msg) => println!("OK: {msg}"),
                Err(e) => {
                    println!("FAILED: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::JoinCode) => {
            let mut settings = common::transport::load_settings();
            settings.premium_enabled = true;
            if !settings.premium_configured() {
                anyhow::bail!(
                    "Premium is not configured: set RRPC_PREMIUM_URL and RRPC_PREMIUM_LICENSE, or use the Premium page in the app."
                );
            }
            let key = common::transport::load_or_create_secret_key();
            match settings.join_code(&common::transport::pairing_id(&key)) {
                Some(code) => println!("{code}"),
                None => anyhow::bail!(
                    settings
                        .premium_blocker()
                        .unwrap_or_else(|| "Premium is not available.".to_string())
                ),
            }
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

fn parse_mode(
    arg: Option<String>,
    default: common::protocol::QualityMode,
) -> Result<common::protocol::QualityMode, anyhow::Error> {
    match arg {
        None => Ok(default),
        Some(s) => common::protocol::QualityMode::parse(&s)
            .ok_or_else(|| anyhow::anyhow!("unknown quality mode \"{s}\" (use auto, smooth, sharp or saver)")),
    }
}

fn launch_unified_gui(
    host: Option<String>,
    password: Option<String>,
    rt: Arc<tokio::runtime::Runtime>,
) -> Result<(), anyhow::Error> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([760.0, 520.0])
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
