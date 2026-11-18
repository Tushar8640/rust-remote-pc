#![allow(dead_code)]
use crate::client::audio::{AudioControl, AudioPlayer};
use crate::common::crypto::hash_password;
use crate::common::protocol::{InputEvent, Packet, PROTOCOL_VERSION};
use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::info;

pub struct ConnectionHandle {
    pub tx_input: mpsc::Sender<InputEvent>,
    pub tx_clipboard: mpsc::Sender<String>,
    pub screen_width: u32,
    pub screen_height: u32,
    pub rtt_ms: Arc<AtomicU64>,
    /// Volume / mute controls. `None` if no audio output device is available.
    pub audio: Option<Arc<AudioControl>>,
}

pub struct DecodedFrameMessage {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Vec<u8>,
    pub timestamp_ms: u64,
}

pub async fn connect_to_host(
    host_addr: &str,
    password: &str,
    frame_sender: mpsc::Sender<DecodedFrameMessage>,
) -> Result<ConnectionHandle> {
    info!("Connecting to host at: {}", host_addr);
    let mut socket = TcpStream::connect(host_addr).await?;

    // 1. Send AuthRequest
    let token_hash = hash_password(password);
    let auth_pkt = Packet::AuthRequest {
        client_version: PROTOCOL_VERSION.to_string(),
        token_hash,
    };

    let auth_bytes = auth_pkt.to_bytes()?;
    socket.write_all(&auth_bytes).await?;
    socket.flush().await?;

    // 2. Await AuthResponse
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut payload = vec![0u8; len];
    socket.read_exact(&mut payload).await?;
    let resp = Packet::from_payload(&payload)?;

    let (screen_width, screen_height) = match resp {
        Packet::AuthResponse {
            success,
            message,
            screen_width,
            screen_height,
        } => {
            if !success {
                return Err(anyhow!("Authentication failed: {}", message));
            }
            info!("Authenticated successfully! Remote screen: {}x{}", screen_width, screen_height);
            (screen_width, screen_height)
        }
        _ => return Err(anyhow!("Unexpected response from server")),
    };

    let (mut socket_read, mut socket_write) = socket.into_split();
    let (tx_outgoing, mut rx_outgoing) = mpsc::channel::<Packet>(128);
    let (tx_input, mut rx_input) = mpsc::channel::<InputEvent>(256);
    let (tx_clipboard, mut rx_clipboard) = mpsc::channel::<String>(16);

    let rtt_ms = Arc::new(AtomicU64::new(0));
    let rtt_clone = rtt_ms.clone();

    // Outgoing packet transmitter
    tokio::spawn(async move {
        while let Some(pkt) = rx_outgoing.recv().await {
            if let Ok(bytes) = pkt.to_bytes() {
                if socket_write.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        }
    });

    // Input events -> Outgoing packets
    let tx_out_input = tx_outgoing.clone();
    tokio::spawn(async move {
        while let Some(event) = rx_input.recv().await {
            let _ = tx_out_input.send(Packet::Input(event)).await;
        }
    });

    // Clipboard -> Outgoing packets
    let tx_out_clip = tx_outgoing.clone();
    tokio::spawn(async move {
        while let Some(text) = rx_clipboard.recv().await {
            let _ = tx_out_clip.send(Packet::Clipboard(text)).await;
        }
    });

    // Periodic Ping for RTT measurement
    let tx_out_ping = tx_outgoing.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if tx_out_ping.send(Packet::Ping(now)).await.is_err() {
                break;
            }
        }
    });

    // Audio playback (optional: the session still works without an output device)
    let (mut audio_player, audio_ctl) = match AudioPlayer::new() {
        Ok((p, c)) => (Some(p), Some(c)),
        Err(e) => {
            tracing::warn!("Audio playback unavailable: {:?}", e);
            (None, None)
        }
    };

    // Incoming packet receiver
    tokio::spawn(async move {
        let mut decoder = crate::client::decoder::VideoDecoder::new();

        loop {
            let mut len_buf = [0u8; 4];
            if socket_read.read_exact(&mut len_buf).await.is_err() {
                info!("Server disconnected");
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut payload = vec![0u8; len];
            if socket_read.read_exact(&mut payload).await.is_err() {
                break;
            }

            if let Ok(packet) = Packet::from_payload(&payload) {
                match packet {
                    Packet::VideoFrame {
                        width,
                        height,
                        format,
                        data,
                        timestamp_ms,
                    } => {
                        if let Ok((w, h, rgba)) = decoder.decode(format, &data, width, height) {
                            let _ = frame_sender.try_send(DecodedFrameMessage {
                                width: w,
                                height: h,
                                rgba_data: rgba,
                                timestamp_ms,
                            });
                        }
                    }
                    Packet::Audio { sample_rate, channels, data } => {
                        if let Some(p) = audio_player.as_mut() {
                            p.push(sample_rate, channels, &data);
                        }
                    }
                    Packet::Pong(ts) => {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        let rtt = now.saturating_sub(ts);
                        rtt_clone.store(rtt, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        }
    });

    Ok(ConnectionHandle {
        tx_input,
        tx_clipboard,
        screen_width,
        screen_height,
        rtt_ms,
        audio: audio_ctl,
    })
}
