//! Premium relay server.
//!
//! An iroh relay that only admits clients presenting a valid premium pass (see the
//! `premium-auth` crate). It speaks plain HTTP/WebSocket on one port, which is what
//! hosts such as Render expect: the platform terminates TLS in front of it.
//!
//! Commands:
//!   premium-relay                 run the relay (default)
//!   premium-relay issue <id>      print a licence key for customer <id>
//!   premium-relay check <pass>    verify a pass (for troubleshooting)
//!
//! Environment:
//!   RELAY_SECRET           required, at least 16 characters. Keep it private; it signs licences.
//!   PORT                   listen port (default 10000, which is Render's default)
//!   REVOKED_LICENSES       comma-separated licence ids to refuse
//!   CLIENT_MAX_KBPS        optional per-client upload cap through the relay, in kbit/s

use anyhow::{anyhow, bail, Context, Result};
use iroh_relay::server::{
    Access, AccessControl, ClientRateLimit, ClientRequest, RelayConfig, Server, ServerConfig,
};
use premium_auth::{issue_license, verify_pass};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn secret_from_env() -> Result<String> {
    let s = std::env::var("RELAY_SECRET").context("RELAY_SECRET is not set")?;
    if s.len() < 16 {
        bail!("RELAY_SECRET must be at least 16 characters");
    }
    Ok(s)
}

/// Admits a connection only if its bearer token is a valid, unexpired, unrevoked pass.
struct PassGate {
    secret: String,
    revoked: HashSet<String>,
}

impl std::fmt::Debug for PassGate {
    // Never print the secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PassGate").field("revoked", &self.revoked.len()).finish()
    }
}

impl AccessControl for PassGate {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        let Some(token) = request.auth_token() else {
            warn!("denied {}: no pass presented", request.endpoint_id().fmt_short());
            return Access::Deny { reason: Some("a premium pass is required".to_string()) };
        };
        match verify_pass(&self.secret, &token, now_secs(), |id| self.revoked.contains(id)) {
            Ok(licence) => {
                info!("admitted {} (licence {licence})", request.endpoint_id().fmt_short());
                Access::Allow
            }
            Err(e) => {
                // Log the reason only, never the token.
                warn!("denied {}: {e}", request.endpoint_id().fmt_short());
                Access::Deny { reason: Some(format!("premium pass rejected: {e}")) }
            }
        }
    }
}

async fn serve() -> Result<()> {
    let secret = secret_from_env()?;
    let port: u16 = match std::env::var("PORT") {
        Ok(p) => p.parse().context("PORT is not a valid port number")?,
        Err(_) => 10000,
    };
    let revoked: HashSet<String> = std::env::var("REVOKED_LICENSES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let mut relay = RelayConfig::new(addr);
    relay.access = Arc::new(PassGate { secret, revoked: revoked.clone() });
    // An empty value (for example from a blank dashboard field) means "no cap".
    if let Some(kbps) = std::env::var("CLIENT_MAX_KBPS").ok().filter(|v| !v.trim().is_empty()) {
        let kbps: u32 = kbps.trim().parse().context("CLIENT_MAX_KBPS must be a number")?;
        let bytes = NonZeroU32::new(kbps.saturating_mul(125)).ok_or_else(|| anyhow!("CLIENT_MAX_KBPS must be above 0"))?;
        let mut limit = ClientRateLimit::new(bytes);
        limit.max_burst_bytes = NonZeroU32::new(bytes.get().saturating_mul(2));
        relay.limits.client_rx = Some(limit);
        info!("per-client cap: {kbps} kbit/s");
    }

    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    config.quic = None; // HTTP-only hosts cannot offer QUIC address discovery
    config.metrics_addr = None;

    let server = Server::spawn(config).await.map_err(|e| anyhow!("failed to start relay: {e:#}"))?;
    info!(
        "premium relay listening on {} ({} revoked licence(s))",
        server.http_addr().map(|a| a.to_string()).unwrap_or_else(|| addr.to_string()),
        revoked.len()
    );

    shutdown_signal().await;
    info!("shutting down");
    let _ = server.shutdown().await;
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => serve().await,
        Some("issue") => {
            let id = args.get(1).ok_or_else(|| anyhow!("usage: premium-relay issue <customer-id>"))?;
            let key = issue_license(&secret_from_env()?, id).map_err(|e| anyhow!("{e}"))?;
            println!("{key}");
            Ok(())
        }
        Some("check") => {
            let pass = args.get(1).ok_or_else(|| anyhow!("usage: premium-relay check <pass>"))?;
            match verify_pass(&secret_from_env()?, pass, now_secs(), |_| false) {
                Ok(id) => println!("valid pass for licence {id}"),
                Err(e) => {
                    println!("rejected: {e}");
                    std::process::exit(1);
                }
            }
            Ok(())
        }
        Some(other) => bail!("unknown command '{other}' (use: serve, issue <id>, check <pass>)"),
    }
}
