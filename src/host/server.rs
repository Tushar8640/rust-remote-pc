use crate::common::crypto::verify_token;
use crate::common::protocol::{FrameFormat, Packet, QualityMode};
use crate::host::capture::ScreenCapturer;
use crate::host::clipboard::HostClipboard;
use crate::host::encoder::VideoEncoder;
use crate::host::input::HostInputExecutor;
use crate::common::transport::{relay_mode_for, PremiumRelay, ALPN};
use anyhow::{anyhow, Result};
use iroh::endpoint::presets;
use iroh::{Endpoint, SecretKey};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

pub struct HostServerConfig {
    pub bind_addr: String,
    pub password: String,
    pub target_fps: u32,
    pub jpeg_quality: u8,
    /// Stream this PC's audio output to connected clients.
    pub audio_enabled: bool,
    /// When set, also accept peer-to-peer (iroh) connections identified by this
    /// key. Works across NAT and the internet without port forwarding or a VPN.
    pub p2p_key: Option<SecretKey>,
    /// Quality mode offered to viewers by default (a viewer can change it for its own session).
    pub quality_mode: QualityMode,
    /// Self-hosted premium relay to use instead of the free public relays.
    pub premium_relay: Option<PremiumRelay>,
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
        info!("Waiting for client connections (direct TCP / Tailscale / P2P)...");

        if let Some(key) = self.config.p2p_key.clone() {
            let password = self.config.password.clone();
            let fps = self.config.target_fps;
            let quality = self.config.jpeg_quality;
            let audio = self.config.audio_enabled;
            let mode = self.config.quality_mode;
            let premium = self.config.premium_relay.clone();
            let p2p_shutdown = shutdown_rx.resubscribe();
            tokio::spawn(async move {
                if let Err(e) = run_p2p(key, password, fps, quality, audio, mode, premium, p2p_shutdown).await {
                    error!("P2P listener stopped: {:?}", e);
                }
            });
        }

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
                            let audio = self.config.audio_enabled;
                            let mode = self.config.quality_mode;

                            let _ = socket.set_nodelay(true);
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(socket, password, fps, quality, audio, mode).await {
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

/// Accepts peer-to-peer connections until `shutdown` fires.
async fn run_p2p(
    key: SecretKey,
    password: String,
    fps: u32,
    quality: u8,
    audio: bool,
    mode: QualityMode,
    premium: Option<PremiumRelay>,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let relay_mode = relay_mode_for(premium.as_ref()).map_err(|e| anyhow!(e))?;
    match &premium {
        Some(p) if p.is_configured() => info!("Using premium relay: {}", p.url.trim()),
        _ => info!("Using public relays"),
    }
    let endpoint = Endpoint::builder(presets::N0)
        .relay_mode(relay_mode)
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .transport_config(crate::common::transport::quic_config())
        .bind()
        .await
        .map_err(|e| anyhow!("failed to start P2P endpoint: {e:#}"))?;
    info!("P2P ready. Pairing ID: {}", endpoint.id());

    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let password = password.clone();
                tokio::spawn(async move {
                    let res: Result<()> = async {
                        let accepting = incoming.accept().map_err(|e| anyhow!("{e:#}"))?;
                        let conn = accepting.await.map_err(|e| anyhow!("{e:#}"))?;
                        info!("Incoming P2P connection from {}", conn.remote_id().fmt_short());
                        let (send, recv) = conn.accept_bi().await.map_err(|e| anyhow!("{e:#}"))?;
                        let r = handle_client(tokio::io::join(recv, send), password, fps, quality, audio, mode).await;
                        // Give the final packet (e.g. "wrong password") time to reach the
                        // client before closing; returns at once if the client already left.
                        let _ = tokio::time::timeout(Duration::from_secs(3), conn.closed()).await;
                        conn.close(0u32.into(), b"done");
                        r
                    }
                    .await;
                    if let Err(e) = res {
                        error!("P2P client connection terminated: {:?}", e);
                    }
                });
            }
        }
    }

    endpoint.close().await;
    Ok(())
}

/// A captured frame on its way to the encoder.
struct CapturedFrame {
    captured_at_ms: u64,
    capture_time: Duration,
    frame: image::RgbaImage,
}

/// Holds only the newest encoded frame. Putting a new one replaces an unsent one.
#[derive(Default)]
struct FrameSlot {
    frame: std::sync::Mutex<Option<Packet>>,
    notify: tokio::sync::Notify,
}

impl FrameSlot {
    /// Stores `p`; returns true if it replaced a frame that was never sent.
    fn put(&self, p: Packet) -> bool {
        let replaced = self.frame.lock().map(|mut g| g.replace(p).is_some()).unwrap_or(false);
        self.notify.notify_one();
        replaced
    }

    fn take(&self) -> Option<Packet> {
        self.frame.lock().ok().and_then(|mut g| g.take())
    }
}

/// Encoding settings, best quality first. `true` = half resolution (about 4x fewer
/// pixels). The sender moves down this ladder when the link cannot keep up and
/// back up when it has headroom, so latency stays low instead of frames queueing.
const LADDER: &[(bool, u8)] = &[
    (false, 95), // 0
    (false, 90), // 1
    (false, 85), // 2
    (false, 75), // 3
    (false, 65), // 4
    (false, 55), // 5
    (true, 70),  // 6
    (true, 55),  // 7
    (true, 40),  // 8
    (true, 30),  // 9
];

struct AdaptState {
    good_frames: u32,
    last_change: std::time::Instant,
    last_up: Option<std::time::Instant>,
    up_wait: Duration,
}

/// Picks a ladder level from how long the link takes to accept each frame.
///
/// The quality mode sets the best level the controller may use (`ceiling`) and
/// how patient it is with slow frames:
/// * Auto: up to the host's quality setting, backs off when a frame takes more than ~1.5 frame intervals.
/// * Smooth: capped at q65 and backs off sooner, favouring frame rate and low delay.
/// * Sharp: up to q90 and tolerates slow frames, favouring picture quality.
/// * Data saver: starts at half resolution.
struct Adaptive {
    level: std::sync::atomic::AtomicUsize,
    ceiling: std::sync::atomic::AtomicUsize,
    mode: std::sync::atomic::AtomicU8,
    user_quality: u8,
    interval: Duration,
    state: std::sync::Mutex<AdaptState>,
}

impl Adaptive {
    fn new(user_quality: u8, mode: QualityMode, interval: Duration) -> Self {
        let a = Self {
            level: std::sync::atomic::AtomicUsize::new(0),
            ceiling: std::sync::atomic::AtomicUsize::new(0),
            mode: std::sync::atomic::AtomicU8::new(0),
            user_quality,
            interval,
            state: std::sync::Mutex::new(AdaptState {
                good_frames: 0,
                last_change: std::time::Instant::now(),
                last_up: None,
                up_wait: Duration::from_secs(3),
            }),
        };
        a.set_mode(mode);
        a
    }

    /// Switches mode (host default at connect, or a viewer's request mid-session).
    fn set_mode(&self, mode: QualityMode) {
        // The host's quality setting is the top of the Auto range.
        let auto = LADDER
            .iter()
            .position(|&(half, q)| !half && q <= self.user_quality)
            .unwrap_or(0);
        let ceiling = match mode {
            QualityMode::Auto => auto,
            QualityMode::Smooth => auto.max(4),
            QualityMode::Sharp => auto.min(1),
            QualityMode::DataSaver => 7,
        };
        self.mode.store(mode as u8, Ordering::Relaxed);
        self.ceiling.store(ceiling, Ordering::Relaxed);
        self.level.store(ceiling, Ordering::Relaxed);
        if let Ok(mut s) = self.state.lock() {
            s.good_frames = 0;
            s.last_change = std::time::Instant::now();
            s.last_up = None;
            s.up_wait = Duration::from_secs(3);
        }
        info!("Video quality mode: {}", mode.label());
    }

    fn level(&self) -> usize {
        self.level.load(Ordering::Relaxed)
    }

    /// A frame taking longer than this to hand to the link counts as a stall.
    fn stall_after(&self) -> Duration {
        let (min_ms, factor) = match QualityMode::from_u8(self.mode.load(Ordering::Relaxed)) {
            QualityMode::Auto => (40.0, 1.5),
            QualityMode::Smooth => (30.0, 1.0),
            QualityMode::Sharp => (100.0, 3.0),
            QualityMode::DataSaver => (60.0, 2.0),
        };
        Duration::from_secs_f64((self.interval.as_secs_f64() * factor).max(min_ms / 1000.0))
    }

    /// `took` is how long writing one frame blocked. A slow write means the
    /// link (or the peer) is the bottleneck, so back off; consistently fast
    /// writes mean there is room to improve quality again.
    fn on_frame_sent(&self, took: Duration) {
        let stall_after = self.stall_after();
        let ceiling = self.ceiling.load(Ordering::Relaxed);
        let Ok(mut s) = self.state.lock() else { return };
        let now = std::time::Instant::now();
        let level = self.level();

        if took > stall_after {
            s.good_frames = 0;
            if level + 1 < LADDER.len() && now.duration_since(s.last_change) >= Duration::from_millis(300) {
                // A recent upgrade that failed right away: wait longer before retrying.
                if s.last_up.is_some_and(|t| now.duration_since(t) < Duration::from_secs(5)) {
                    s.up_wait = (s.up_wait * 2).min(Duration::from_secs(30));
                }
                self.level.store(level + 1, Ordering::Relaxed);
                s.last_change = now;
                let (half, q) = LADDER[level + 1];
                info!(
                    "Link slow ({} ms/frame): lowering video to {}q{}",
                    took.as_millis(),
                    if half { "half-res " } else { "" },
                    q
                );
            }
        } else if took < stall_after.mul_f32(0.4) {
            s.good_frames += 1;
            if now.duration_since(s.last_change) > Duration::from_secs(20) {
                s.up_wait = Duration::from_secs(3);
            }
            if level > ceiling && s.good_frames >= 15 && now.duration_since(s.last_change) >= s.up_wait {
                self.level.store(level - 1, Ordering::Relaxed);
                s.last_change = now;
                s.last_up = Some(now);
                s.good_frames = 0;
                let (half, q) = LADDER[level - 1];
                info!("Link has headroom: raising video to {}q{}", if half { "half-res " } else { "" }, q);
            }
        } else {
            s.good_frames = 0;
        }
    }
}

/// Encodes captured frames and publishes the newest one. Skips frames identical
/// to the last one sent, so an idle desktop costs almost nothing.
fn encode_loop(
    rx: std::sync::mpsc::Receiver<CapturedFrame>,
    slot: Arc<FrameSlot>,
    adapt: Arc<Adaptive>,
    stop: Arc<AtomicBool>,
) {
    let mut encoder = VideoEncoder::new(FrameFormat::Jpeg { quality: 75 });
    let mut prev: Vec<u8> = Vec::new();
    let mut last_level = usize::MAX;
    let mut last_sent = std::time::Instant::now();
    let mut st = VideoStats::default();

    while let Ok(cf) = rx.recv() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let (w, h) = (cf.frame.width(), cf.frame.height());
        let raw = cf.frame.into_raw();
        let level = adapt.level();

        // Unchanged picture: nothing to send (but refresh every 2 s, and whenever
        // quality changed so a static screen still gets sharper).
        if raw == prev && level == last_level && last_sent.elapsed() < Duration::from_secs(2) {
            st.skipped += 1;
            st.tick(level);
            continue;
        }

        let (half, quality) = LADDER[level.min(LADDER.len() - 1)];
        let t0 = std::time::Instant::now();
        match encoder.encode(w, h, &raw, quality, half) {
            Ok(enc) => {
                st.record(cf.capture_time, t0.elapsed(), enc.data.len());
                let packet = Packet::VideoFrame {
                    width: enc.width,
                    height: enc.height,
                    format: enc.format,
                    data: enc.data,
                    timestamp_ms: cf.captured_at_ms,
                };
                if slot.put(packet) {
                    st.stale_dropped += 1;
                }
                prev = raw;
                last_level = level;
                last_sent = std::time::Instant::now();
            }
            Err(e) => warn!("Frame encoding failed: {:?}", e),
        }
        st.tick(level);
    }
}

/// Rolling video pipeline statistics, logged periodically.
struct VideoStats {
    since: std::time::Instant,
    frames: u32,
    stale_dropped: u32,
    skipped: u32,
    capture: Duration,
    encode: Duration,
    bytes: usize,
}

impl Default for VideoStats {
    fn default() -> Self {
        Self {
            since: std::time::Instant::now(),
            frames: 0,
            stale_dropped: 0,
            skipped: 0,
            capture: Duration::ZERO,
            encode: Duration::ZERO,
            bytes: 0,
        }
    }
}

impl VideoStats {
    fn record(&mut self, capture: Duration, encode: Duration, bytes: usize) {
        self.frames += 1;
        self.capture += capture;
        self.encode += encode;
        self.bytes += bytes;
    }

    fn tick(&mut self, level: usize) {
        let secs = self.since.elapsed().as_secs_f64();
        if secs < 3.0 {
            return;
        }
        let n = self.frames.max(1) as f64;
        let (half, q) = LADDER[level.min(LADDER.len() - 1)];
        info!(
            "video: {:.1} fps | capture {:.1} ms | encode {:.1} ms | {:.0} KB/frame | {:.1} Mbit/s | {}q{} | stale-dropped {} | unchanged-skipped {}",
            self.frames as f64 / secs,
            self.capture.as_secs_f64() * 1000.0 / n,
            self.encode.as_secs_f64() * 1000.0 / n,
            self.bytes as f64 / n / 1024.0,
            self.bytes as f64 * 8.0 / secs / 1e6,
            if half { "half-res " } else { "" },
            q,
            self.stale_dropped,
            self.skipped
        );
        *self = Self::default();
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

async fn handle_client<S>(
    mut socket: S,
    expected_password: String,
    target_fps: u32,
    jpeg_quality: u8,
    audio_enabled: bool,
    quality_mode: QualityMode,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
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
    let (mut socket_read, mut socket_write) = tokio::io::split(socket);

    // Set when the session ends so the blocking capture thread exits too.
    // (Aborting a spawn_blocking task does not stop it.)
    let stop = Arc::new(AtomicBool::new(false));

    // Control packets (pong, audio, clipboard) use a small queue that always takes
    // priority. Video does NOT use a queue: it goes through a one-slot mailbox that
    // only ever holds the newest frame, so a slow link drops stale frames instead
    // of building seconds of delay.
    let (tx_outgoing, mut rx_outgoing) = mpsc::channel::<Packet>(32);
    let slot = Arc::new(FrameSlot::default());
    let adapt = Arc::new(Adaptive::new(
        jpeg_quality,
        quality_mode,
        Duration::from_millis((1000 / target_fps.max(1) as u64).max(10)),
    ));
    // Test hook: RRPC_SIM_KBPS=<kbit/s> emulates a slow link on the video path.
    let sim_kbps: u64 = std::env::var("RRPC_SIM_KBPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // Task 1: write packets to the socket
    let write_slot = slot.clone();
    let write_adapt = adapt.clone();
    let write_task = tokio::spawn(async move {
        loop {
            let packet = tokio::select! {
                biased;
                msg = rx_outgoing.recv() => match msg {
                    Some(p) => p,
                    None => break,
                },
                _ = write_slot.notify.notified() => match write_slot.take() {
                    Some(p) => p,
                    None => continue,
                },
            };
            let video_len = match &packet {
                Packet::VideoFrame { data, .. } => Some(data.len()),
                _ => None,
            };
            let started = std::time::Instant::now();
            if let Err(e) = write_packet(&mut socket_write, &packet).await {
                error!("Socket write error: {:?}", e);
                break;
            }
            if let Some(len) = video_len {
                if sim_kbps > 0 {
                    tokio::time::sleep(Duration::from_micros(len as u64 * 8000 / sim_kbps)).await;
                }
                // How long the link took to accept this frame drives quality adaptation.
                write_adapt.on_frame_sent(started.elapsed());
            }
        }
    });

    // Task 2: screen capture (this thread) -> encoder thread -> frame mailbox.
    // Capture and encoding overlap, so a frame costs max(capture, encode), not the sum.
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<CapturedFrame>(0);
    {
        let slot = slot.clone();
        let adapt = adapt.clone();
        let stop = stop.clone();
        let _ = std::thread::Builder::new()
            .name("encoder".into())
            .spawn(move || encode_loop(frame_rx, slot, adapt, stop));
    }

    let stop_capture = stop.clone();
    let capture_task = tokio::task::spawn_blocking(move || {
        let capturer = match ScreenCapturer::new() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to create ScreenCapturer in capture task: {:?}", e);
                return;
            }
        };
        let interval = Duration::from_millis((1000 / target_fps.max(1) as u64).max(10));

        while !stop_capture.load(Ordering::Relaxed) {
            let start = std::time::Instant::now();
            // Frame timestamp = moment of capture, so the client can measure true
            // end-to-end latency (capture + encode + network + decode).
            let captured_at_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            match capturer.capture() {
                Ok(frame) => {
                    let cf = CapturedFrame { captured_at_ms, capture_time: start.elapsed(), frame };
                    // Blocks until the encoder is free; Err means the session ended.
                    if frame_tx.send(cf).is_err() {
                        break;
                    }
                }
                Err(e) => warn!("Capture error: {:?}", e),
            }

            let elapsed = start.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
    });

    // Audio: capture this PC's output and stream it. Held until the client
    // disconnects; dropping it stops the capture thread.
    let _audio_capture = if audio_enabled {
        match crate::host::audio::AudioCapture::start(tx_outgoing.clone()) {
            Ok(a) => Some(a),
            Err(e) => {
                warn!("Audio streaming unavailable: {:?}", e);
                None
            }
        }
    } else {
        None
    };

    // Task 3: Input execution and incoming packet reader
    let mut input_executor = HostInputExecutor::new(screen_width, screen_height)?;
    let mut clipboard = HostClipboard::new().ok();
    let tx_resp = tx_outgoing.clone();
    let adapt_read = adapt.clone();

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
                    Packet::SetQuality { mode } => {
                        adapt_read.set_mode(QualityMode::from_u8(mode));
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
    let mut read_task = read_task;
    let mut write_task = write_task;
    tokio::select! {
        _ = &mut read_task => {
            info!("Client read task finished");
        }
        _ = &mut write_task => {
            info!("Client write task finished");
        }
    }

    stop.store(true, Ordering::Relaxed);
    read_task.abort();
    write_task.abort();
    capture_task.abort();
    Ok(())
}
