use crate::common::crypto::verify_token;
use crate::common::protocol::{FrameFormat, Packet};
use crate::host::capture::ScreenCapturer;
use crate::host::clipboard::HostClipboard;
use crate::host::encoder::VideoEncoder;
use crate::host::input::HostInputExecutor;
use anyhow::{anyhow, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub struct HostServerConfig {
    pub bind_addr: String,
    pub password: String,
    pub target_fps: u32,
    pub jpeg_quality: u8,
}

pub struct HostServer {
    config: HostServerConfig,
}

impl HostServer {
    pub fn new(config: HostServerConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<()> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        self.run_with_shutdown(rx).await
    }

    pub async fn run_with_shutdown(
        &self,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        let listener = TcpListener::bind(&self.config.bind_addr).await?;
        info!("Host Server listening on: {}", self.config.bind_addr);
        info!("Waiting for Client connection over Tailscale...");

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Host Server received shutdown signal. Stopping.");
                    break;
                }
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((socket, addr)) => {
                            info!("Incoming connection from: {}", addr);
                            let password = self.config.password.clone();
                            let fps = self.config.target_fps;
                            let quality = self.config.jpeg_quality;

                            tokio::spawn(async move {
                                if let Err(e) = handle_client(socket, password, fps, quality).await {
                                    error!("Client connection terminated: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Error accepting connection: {:?}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn read_packet(socket: &mut (impl AsyncReadExt + Unpin)) -> Result<Packet> {
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 50 * 1024 * 1024 {
        return Err(anyhow!("Packet size too large: {} bytes", len));
    }

    let mut payload = vec![0u8; len];
    socket.read_exact(&mut payload).await?;
    let packet = Packet::from_payload(&payload)?;
    Ok(packet)
}

async fn write_packet(socket: &mut (impl AsyncWriteExt + Unpin), packet: &Packet) -> Result<()> {
    let bytes = packet.to_bytes()?;
    socket.write_all(&bytes).await?;
    socket.flush().await?;
    Ok(())
}

async fn handle_client(
    mut socket: TcpStream,
    expected_password: String,
    target_fps: u32,
    jpeg_quality: u8,
) -> Result<()> {
    // 1. Initial Handshake & Authentication
    let first_packet = read_packet(&mut socket).await?;

    let (client_version, token_hash) = match first_packet {
        Packet::AuthRequest {
            client_version,
            token_hash,
        } => (client_version, token_hash),
        _ => {
            write_packet(
                &mut socket,
                &Packet::AuthResponse {
                    success: false,
                    message: "Expected AuthRequest".to_string(),
                    screen_width: 0,
                    screen_height: 0,
                },
            )
            .await?;
            return Err(anyhow!("Handshake failed: Invalid initial packet"));
        }
    };

    if !verify_token(&expected_password, &token_hash) {
        warn!("Authentication failed from client (version: {})", client_version);
        write_packet(
            &mut socket,
            &Packet::AuthResponse {
                success: false,
                message: "Authentication failed: Invalid password / token".to_string(),
                screen_width: 0,
                screen_height: 0,
            },
        )
        .await?;
        return Err(anyhow!("Authentication failed"));
    }

    // Initialize capturer and input executor
    let capturer = ScreenCapturer::new()?;
    let (screen_width, screen_height) = capturer.dimensions();

    info!(
        "Client authenticated successfully! Primary display: {}x{}",
        screen_width, screen_height
    );

    write_packet(
        &mut socket,
        &Packet::AuthResponse {
            success: true,
            message: "Authentication successful".to_string(),
            screen_width,
            screen_height,
        },
    )
    .await?;

    // Split socket into Read and Write halves
    let (mut socket_read, mut socket_write) = socket.into_split();

    // Channel for outgoing packets (video frames, clipboard, pong)
    let (tx_outgoing, mut rx_outgoing) = mpsc::channel::<Packet>(128);

    // Task 1: Write outgoing packets to socket
    let write_task = tokio::spawn(async move {
        while let Some(packet) = rx_outgoing.recv().await {
            if let Err(e) = write_packet(&mut socket_write, &packet).await {
                error!("Socket write error: {:?}", e);
                break;
            }
        }
    });

    // Task 2: Screen capture & video stream generator
    let tx_video = tx_outgoing.clone();

    let capture_task = tokio::task::spawn_blocking(move || {
        let capturer = match ScreenCapturer::new() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to create ScreenCapturer in capture task: {:?}", e);
                return;
            }
        };
        let mut encoder = VideoEncoder::new(FrameFormat::Jpeg { quality: jpeg_quality });
        let interval_ms = (1000 / target_fps.max(1)).max(10);

        loop {
            let start = std::time::Instant::now();

            match capturer.capture() {
                Ok(frame) => {
                    let w = frame.width();
                    let h = frame.height();
                    let raw_bytes = frame.into_raw();

                    match encoder.encode(w, h, &raw_bytes) {
                        Ok((format, compressed)) => {
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;

                            let packet = Packet::VideoFrame {
                                width: w,
                                height: h,
                                format,
                                data: compressed,
                                timestamp_ms: now,
                            };

                            // Non-blocking send; if queue is full, skip frame to keep latency minimal!
                            let _ = tx_video.try_send(packet);
                        }
                        Err(e) => {
                            warn!("Frame encoding failed: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Capture error: {:?}", e);
                }
            }

            let elapsed = start.elapsed();
            if elapsed < Duration::from_millis(interval_ms as u64) {
                std::thread::sleep(Duration::from_millis(interval_ms as u64) - elapsed);
            }
        }
    });

    // Task 3: Input execution and incoming packet reader
    let mut input_executor = HostInputExecutor::new(screen_width, screen_height)?;
    let mut clipboard = HostClipboard::new().ok();
    let tx_resp = tx_outgoing.clone();

    let read_task = tokio::spawn(async move {
        loop {
            match read_packet(&mut socket_read).await {
                Ok(packet) => match packet {
                    Packet::Input(input_event) => {
                        input_executor.execute(input_event);
                    }
                    Packet::Clipboard(text) => {
                        if let Some(cb) = &mut clipboard {
                            cb.set_text(&text);
                        }
                    }
                    Packet::Ping(timestamp) => {
                        let _ = tx_resp.send(Packet::Pong(timestamp)).await;
                    }
                    Packet::Pong(_) => {}
                    _ => {}
                },
                Err(e) => {
                    info!("Client disconnected or read error: {:?}", e);
                    break;
                }
            }
        }
    });

    // Wait until reader finishes or fails
    tokio::select! {
        _ = read_task => {
            info!("Client read task finished");
        }
        _ = write_task => {
            info!("Client write task finished");
        }
    }

    capture_task.abort();
    Ok(())
}
