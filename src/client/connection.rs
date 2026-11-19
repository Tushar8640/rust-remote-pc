#![allow(dead_code)]
use crate::client::audio::{AudioControl, AudioPlayer};
use crate::common::crypto::hash_password;
use crate::common::protocol::{InputEvent, Packet, QualityMode, PROTOCOL_VERSION};
use crate::common::transport::{is_pairing_id, relay_mode_for, resolve_target, PremiumRelay, ALPN};
use anyhow::{anyhow, Result};
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::info;

/// Options for a new connection.
#[derive(Clone, Default)]
pub struct ConnectOptions {
    /// Premium relay and pass to use. Normally taken from a join code, so callers
    /// leave this unset.
    pub premium: Option<PremiumRelay>,
    /// Video quality mode to request from the host.
    pub quality: QualityMode,
}

pub struct ConnectionHandle {
    pub tx_input: mpsc::Sender<InputEvent>,
    pub tx_clipboard: mpsc::Sender<String>,
    /// Ask the host to switch video quality mode for this session.
    pub tx_quality: mpsc::Sender<QualityMode>,
    /// Human-readable link description: "Direct 12 ms", "Relay 85 ms" or "TCP".
    pub link: Arc<std::sync::Mutex<String>>,
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

type BoxRead = Box<dyn AsyncRead + Send + Unpin>;
type BoxWrite = Box<dyn AsyncWrite + Send + Unpin>;
/// Keeps the transport's background resources (P2P endpoint/connection) alive
/// for as long as the session runs.
type KeepAlive = Box<dyn std::any::Any + Send>;

/// Closes the QUIC connection and endpoint gracefully when the session ends.
struct P2pGuard {
    endpoint: Endpoint,
    conn: iroh::endpoint::Connection,
}

impl Drop for P2pGuard {
    fn drop(&mut self) {
        let endpoint = self.endpoint.clone();
        let conn = self.conn.clone();
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move {
                conn.close(0u32.into(), b"bye");
                endpoint.close().await;
            });
        }
    }
}

/// How long to wait for a P2P connection (hole-punching / relay setup).
const P2P_CONNECT_TIMEOUT: Duration = Duration::from_secs(45);

/// Opens a byte stream to the host. `addr` is either `host:port` (direct TCP,
/// LAN or Tailscale) or a 64-character Pairing ID (peer-to-peer over the
/// internet, end-to-end encrypted).
async fn open_transport(
    addr: &str,
    premium: Option<&PremiumRelay>,
) -> Result<(BoxRead, BoxWrite, KeepAlive, Arc<std::sync::Mutex<String>>)> {
    if is_pairing_id(addr) {
        let id: iroh::EndpointId = addr
            .trim()
            .parse()
            .map_err(|e| anyhow!("Invalid Pairing ID: {e}"))?;
        info!("Connecting to host {} over P2P...", id.fmt_short());
        let relay_mode = relay_mode_for(premium).map_err(|e| anyhow!(e))?;
        // With a premium relay we already know where the host is reachable, so dial
        // through it directly instead of depending on public address lookup.
        let target_addr = match premium.filter(|p| p.is_configured()) {
            Some(p) => EndpointAddr::from_parts(id, [iroh::TransportAddr::Relay(p.relay_url().map_err(|e| anyhow!(e))?)]),
            None => EndpointAddr::new(id),
        };

        let fut = async {
            let endpoint = Endpoint::builder(presets::N0)
                .relay_mode(relay_mode)
                .transport_config(crate::common::transport::quic_config())
                .bind()
                .await
                .map_err(|e| anyhow!("Could not start P2P networking: {e:#}"))?;
            let conn = endpoint
                .connect(target_addr, ALPN)
                .await
                .map_err(|e| anyhow!("Could not reach the PC: {e:#}"))?;
            let (send, recv) = conn
                .open_bi()
                .await
                .map_err(|e| anyhow!("Could not open a stream to the PC: {e:#}"))?;
            Ok::<_, anyhow::Error>((endpoint, conn, send, recv))
        };
        let (endpoint, conn, send, recv) = tokio::time::timeout(P2P_CONNECT_TIMEOUT, fut)
            .await
            .map_err(|_| {
                if premium.is_some() {
                    anyhow!("Could not reach the PC through the premium relay. The join code may have expired or been rejected: ask the host for a fresh one, and check sharing is on at the PC.")
                } else {
                    anyhow!("Timed out reaching the PC. Check the Pairing ID and that sharing is on at the PC.")
                }
            })??;
        let link = Arc::new(std::sync::Mutex::new("P2P".to_string()));
        {
            let (link, conn) = (link.clone(), conn.clone());
            tokio::spawn(async move {
                loop {
                    if let Ok(mut l) = link.lock() {
                        *l = describe_path(&conn);
                    }
                    tokio::select! {
                        _ = conn.closed() => break,
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    }
                }
            });
        }
        Ok((Box::new(recv), Box::new(send), Box::new(P2pGuard { endpoint, conn }), link))
    } else {
        info!("Connecting to host at: {}", addr);
        let socket = TcpStream::connect(addr).await?;
        let _ = socket.set_nodelay(true);
        let (r, w) = socket.into_split();
        Ok((Box::new(r), Box::new(w), Box::new(()), Arc::new(std::sync::Mutex::new("TCP".to_string()))))
    }
}

pub async fn connect_to_host(
    host_addr: &str,
    password: &str,
    frame_sender: mpsc::Sender<DecodedFrameMessage>,
    opts: ConnectOptions,
) -> Result<ConnectionHandle> {
    // An invite carries the host's Pairing ID, its password and, for a premium host, the relay
    // (plus a pass) to meet on.
    let resolved = resolve_target(host_addr).map_err(|e| anyhow!(e))?;
    let target = resolved.address;
    let premium = resolved.premium.or(opts.premium.clone());
    let password = resolved.password.as_deref().unwrap_or(password);
    let (mut socket_read, mut socket_write, keepalive, link) =
        open_transport(&target, premium.as_ref()).await?;

    // 1. Send AuthRequest
    let token_hash = hash_password(password);
    let auth_pkt = Packet::AuthRequest {
        client_version: PROTOCOL_VERSION.to_string(),
        token_hash,
    };

    let auth_bytes = auth_pkt.to_bytes()?;
    socket_write.write_all(&auth_bytes).await?;
    socket_write.flush().await?;

    // 2. Await AuthResponse
    let mut len_buf = [0u8; 4];
    socket_read.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 1024 * 1024 {
        return Err(anyhow!("Unexpected response from server"));
    }

    let mut payload = vec![0u8; len];
    socket_read.read_exact(&mut payload).await?;
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

    let (tx_outgoing, mut rx_outgoing) = mpsc::channel::<Packet>(128);
    let (tx_input, mut rx_input) = mpsc::channel::<InputEvent>(256);
    let (tx_clipboard, mut rx_clipboard) = mpsc::channel::<String>(16);
    let (tx_quality, mut rx_quality) = mpsc::channel::<QualityMode>(4);

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

    // Quality mode requests -> Outgoing packets (the initial request goes first)
    let tx_out_quality = tx_outgoing.clone();
    let initial_quality = opts.quality;
    tokio::spawn(async move {
        if initial_quality != QualityMode::Auto {
            let _ = tx_out_quality
                .send(Packet::SetQuality { mode: initial_quality as u8 })
                .await;
        }
        while let Some(m) = rx_quality.recv().await {
            let _ = tx_out_quality.send(Packet::SetQuality { mode: m as u8 }).await;
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
        let _keepalive = keepalive;
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
        tx_quality,
        link,
        screen_width,
        screen_height,
        rtt_ms,
        audio: audio_ctl,
    })
}

/// Describes the path the connection currently uses, e.g. "Direct 12 ms" or "Relay 85 ms".
fn describe_path(conn: &iroh::endpoint::Connection) -> String {
    let paths = conn.paths();
    match paths.iter().find(|p| p.is_selected()) {
        Some(p) => format!(
            "{} {} ms",
            if p.is_relay() { "Relay" } else { "Direct" },
            p.rtt().as_millis()
        ),
        None => "Connecting".to_string(),
    }
}
