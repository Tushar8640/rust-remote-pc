//! Android JNI bridge.
//!
//! The Kotlin client already speaks the remote-desktop protocol over a plain
//! TCP socket. Rather than re-implementing peer-to-peer networking in Kotlin,
//! this library dials the host over iroh (QUIC, hole-punching with relay
//! fallback, end-to-end encrypted) and exposes the resulting stream on a
//! loopback TCP port. Kotlin then connects to `127.0.0.1:<port>` exactly as it
//! would to a LAN host.
//!
//! Other apps on the device can reach loopback ports, so the first 32 bytes the
//! Kotlin side sends must equal a one-time random secret returned from
//! `nativeConnect`; connections that fail this check are dropped. The host
//! password is still verified by the host itself.

use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayConfig, RelayMap, RelayMode, RelayUrl, TransportAddr};
use jni::objects::{JObject, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

const ALPN: &[u8] = b"rust-remote-pc/1";
const SECRET_LEN: usize = 32;
const P2P_CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
const LOCAL_ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);

/// Same QUIC tuning as the desktop app (see `common::transport::quic_config`):
/// small windows so a slow link applies back-pressure instead of queueing seconds
/// of video.
fn quic_config() -> iroh::endpoint::QuicTransportConfig {
    use iroh::endpoint::{QuicTransportConfig, VarInt};
    QuicTransportConfig::builder()
        .send_window(256 * 1024)
        .stream_receive_window(VarInt::from_u32(256 * 1024))
        .receive_window(VarInt::from_u32(512 * 1024))
        .build()
}

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to start tokio runtime")
    })
}

/// Relay selection: an empty URL means the free public relays; otherwise the
/// self-hosted premium relay at `url`, authenticating with `key` if non-empty.
fn relay_mode(url: &str, key: &str) -> Result<RelayMode, String> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(RelayMode::Default);
    }
    let url: RelayUrl = url.parse().map_err(|e| format!("Invalid relay URL: {e}"))?;
    // No QUIC address discovery: the premium relay is an HTTP service behind a proxy.
    let mut cfg = RelayConfig::new(url, None);
    if !key.trim().is_empty() {
        cfg = cfg.with_auth_token(key.trim());
    }
    Ok(RelayMode::Custom(RelayMap::from(cfg)))
}

/// Dials `id` over iroh and starts the loopback bridge. Returns `(port, secret)`.
fn connect(id: &str, relay_url: &str, relay_key: &str) -> Result<(u16, String), String> {
    let relay_mode = relay_mode(relay_url, relay_key)?;
    // With a premium relay we know where the host is reachable, so dial through it directly.
    let relay_hint: Option<RelayUrl> = match relay_url.trim() {
        "" => None,
        u => Some(u.parse().map_err(|e| format!("Invalid relay URL: {e}"))?),
    };
    let relay_hint_set = relay_hint.is_some();
    let id: EndpointId = id
        .trim()
        .parse()
        .map_err(|e| format!("Invalid Pairing ID: {e}"))?;

    let rt = runtime();
    rt.block_on(async {
        let endpoint = Endpoint::builder(presets::N0)
            .relay_mode(relay_mode)
            .transport_config(quic_config())
            .bind()
            .await
            .map_err(|e| format!("Could not start P2P networking: {e:#}"))?;

        let conn = match tokio::time::timeout(
            P2P_CONNECT_TIMEOUT,
            endpoint.connect(
                match relay_hint {
                    Some(u) => EndpointAddr::from_parts(id, [TransportAddr::Relay(u)]),
                    None => EndpointAddr::new(id),
                },
                ALPN,
            ),
        )
        .await
        {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                endpoint.close().await;
                return Err(format!("Could not reach the PC: {e:#}"));
            }
            Err(_) => {
                endpoint.close().await;
                return Err(
                    if relay_hint_set {
                        "Could not reach the PC through the premium relay. The join code may have expired: ask the host for a fresh one."
                    } else {
                        "Timed out reaching the PC. Check the Pairing ID and that sharing is on."
                    }
                    .to_string(),
                );
            }
        };

        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| format!("Could not open a stream to the PC: {e:#}"))?;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Could not open local port: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("Could not open local port: {e}"))?
            .port();

        let mut raw = [0u8; SECRET_LEN / 2];
        getrandom::getrandom(&mut raw).map_err(|e| format!("No randomness available: {e}"))?;
        let secret = hex::encode(raw);

        tokio::spawn(bridge(listener, secret.clone(), endpoint, conn, send, recv));
        Ok((port, secret))
    })
}

async fn bridge(
    listener: TcpListener,
    secret: String,
    endpoint: Endpoint,
    conn: iroh::endpoint::Connection,
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
) {
    let accepted = tokio::time::timeout(LOCAL_ACCEPT_TIMEOUT, async {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return None;
            };
            let mut buf = [0u8; SECRET_LEN];
            let ok = matches!(
                tokio::time::timeout(Duration::from_secs(5), sock.read_exact(&mut buf)).await,
                Ok(Ok(_))
            ) && constant_time_eq(&buf, secret.as_bytes());
            if ok {
                let _ = sock.set_nodelay(true);
                return Some(sock);
            }
            // Wrong or missing secret: drop it and keep waiting for ours.
        }
    })
    .await;
    drop(listener);

    if let Ok(Some(mut sock)) = accepted {
        let mut peer = tokio::io::join(recv, send);
        let _ = tokio::io::copy_bidirectional(&mut sock, &mut peer).await;
        let _ = sock.shutdown().await;
    }

    conn.close(0u32.into(), b"bye");
    endpoint.close().await;
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    match env.new_string(s) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// `String IrohBridge.nativeConnect(String pairingId, String relayUrl, String relayKey)`
///
/// Returns `"OK <port> <secret>"` on success or `"ERR <message>"` on failure.
#[no_mangle]
pub extern "system" fn Java_com_remote_rustpc_network_IrohBridge_nativeConnect(
    mut env: JNIEnv,
    _this: JObject,
    pairing_id: JString,
    relay_url: JString,
    relay_key: JString,
) -> jstring {
    let id: String = match env.get_string(&pairing_id) {
        Ok(s) => s.into(),
        Err(_) => return to_jstring(&mut env, "ERR Invalid Pairing ID"),
    };

    let relay_url: String = env.get_string(&relay_url).map(|s| s.into()).unwrap_or_default();
    let relay_key: String = env.get_string(&relay_key).map(|s| s.into()).unwrap_or_default();

    let result = std::panic::catch_unwind(|| connect(&id, &relay_url, &relay_key))
        .unwrap_or_else(|_| Err("Internal error in P2P layer".to_string()));

    let reply = match result {
        Ok((port, secret)) => format!("OK {port} {secret}"),
        Err(e) => format!("ERR {e}"),
    };
    to_jstring(&mut env, &reply)
}
