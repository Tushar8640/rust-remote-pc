//! Peer-to-peer transport helpers (iroh).
//!
//! The host is identified by an Ed25519 key. Its public key (the "Pairing ID",
//! 64 lowercase hex characters) is all a client needs to reach it from any
//! network: iroh hole-punches a direct QUIC connection and falls back to relay
//! servers when that fails. Traffic is end-to-end encrypted by QUIC/TLS and the
//! host's identity is authenticated by the key itself.

use iroh::SecretKey;
use std::path::PathBuf;
use tracing::{info, warn};

/// Application protocol identifier negotiated during the QUIC handshake.
pub const ALPN: &[u8] = b"rust-remote-pc/1";

/// Length of a Pairing ID in characters (hex encoding of a 32-byte key).
pub const PAIRING_ID_LEN: usize = 64;

fn config_dir() -> Option<PathBuf> {
    if let Some(base) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(base).join("rust-remote-pc"));
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(base).join("rust-remote-pc"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("rust-remote-pc"))
}

/// Loads this machine's persistent host key, creating it on first use.
///
/// Keeping the key stable means the Pairing ID never changes, so a phone can
/// save the PC once and reconnect with one tap. The file holds the private key
/// and must not be shared; it is never logged or displayed.
pub fn load_or_create_secret_key() -> SecretKey {
    let path = config_dir().map(|d| d.join("host.key"));

    if let Some(p) = &path {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
                return SecretKey::from_bytes(&arr);
            }
            warn!("host.key is not a valid key file; generating a new one");
        }
    }

    let key = SecretKey::generate();
    if let Some(p) = &path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(p, key.to_bytes()) {
            Ok(()) => info!("Created new host identity at {}", p.display()),
            Err(e) => warn!("Could not save host identity ({e}); Pairing ID will change on restart"),
        }
    }
    key
}

/// The Pairing ID shown to the user for a host key.
pub fn pairing_id(key: &SecretKey) -> String {
    key.public().to_string()
}

/// True if `s` looks like a Pairing ID (64 hex characters) rather than `host:port`.
pub fn is_pairing_id(s: &str) -> bool {
    let t = s.trim();
    t.len() == PAIRING_ID_LEN && t.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Shortens a Pairing ID for display, leaves other addresses untouched.
pub fn display_addr(s: &str) -> String {
    if is_pairing_id(s) {
        format!("{}…{}", &s[..8], &s[PAIRING_ID_LEN - 6..])
    } else {
        s.to_string()
    }
}

/// QUIC settings tuned for live video.
///
/// By default QUIC will buffer several MB of unacknowledged data per connection.
/// For a screen stream that is pure latency: on a slow link the picture would lag
/// by seconds. Small windows make the sender feel back-pressure quickly, so it
/// can lower quality instead of queueing. The windows still cover the
/// bandwidth-delay product of typical internet paths (about 25 Mbit/s at 80 ms).
pub fn quic_config() -> iroh::endpoint::QuicTransportConfig {
    use iroh::endpoint::{QuicTransportConfig, VarInt};
    QuicTransportConfig::builder()
        .send_window(256 * 1024)
        .stream_receive_window(VarInt::from_u32(256 * 1024))
        .receive_window(VarInt::from_u32(512 * 1024))
        .build()
}

// ─────────────────────────────────────────────────────────────────────────────
// Premium relay, join codes and saved settings
// ─────────────────────────────────────────────────────────────────────────────

use crate::common::protocol::QualityMode;
use premium_auth::{looks_like_join_code, mint_pass, parse_license, JoinCode, Kind, Status};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Premium relay URL compiled into this build (set `RRPC_PREMIUM_RELAY_URL` when building).
/// It pre-fills the Premium page so customers only paste their licence key.
pub const DEFAULT_PREMIUM_URL: &str = match option_env!("RRPC_PREMIUM_RELAY_URL") {
    Some(u) => u,
    None => "",
};

/// How long the host's own relay pass lasts. The host mints a fresh one each time it
/// starts sharing, so this only has to outlast one run.
const HOST_PASS_TTL: u64 = 30 * 24 * 3600;
/// How long a pass inside a join code stays valid.
const JOIN_PASS_TTL: u64 = 24 * 3600;

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A relay to use together with the pass that admits us to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PremiumRelay {
    pub url: String,
    /// The relay pass (bearer token). Never the licence key.
    pub key: String,
}

impl PremiumRelay {
    pub fn is_configured(&self) -> bool {
        !self.url.trim().is_empty()
    }

    /// The parsed relay URL.
    pub fn relay_url(&self) -> Result<iroh::RelayUrl, String> {
        self.url.trim().parse().map_err(|e| format!("Invalid relay URL: {e}"))
    }

    /// The iroh relay mode for this relay.
    ///
    /// QUIC address discovery is switched off: the premium relay is an HTTP/WebSocket
    /// service (it runs behind a platform proxy), so probing its QUIC port would only
    /// slow down start-up.
    pub fn relay_mode(&self) -> Result<iroh::RelayMode, String> {
        let cfg = iroh::RelayConfig::new(self.relay_url()?, None);
        let cfg = if self.key.trim().is_empty() { cfg } else { cfg.with_auth_token(self.key.trim()) };
        Ok(iroh::RelayMode::Custom(iroh::RelayMap::from(cfg)))
    }
}

/// Relay mode to use: the premium relay if one is given, else the public default.
pub fn relay_mode_for(premium: Option<&PremiumRelay>) -> Result<iroh::RelayMode, String> {
    match premium {
        Some(p) if p.is_configured() => p.relay_mode(),
        _ => Ok(iroh::RelayMode::Default),
    }
}

/// Checks that the relay is reachable and accepts the pass, by registering a
/// throwaway endpoint with it. Used by the "Test connection" button.
pub async fn test_premium_relay(p: &PremiumRelay) -> Result<String, String> {
    use iroh::endpoint::presets;
    let mode = p.relay_mode()?;
    let started = std::time::Instant::now();
    let endpoint = iroh::Endpoint::builder(presets::N0)
        .relay_mode(mode)
        .bind()
        .await
        .map_err(|e| format!("Could not start networking: {e:#}"))?;
    let online = tokio::time::timeout(Duration::from_secs(15), endpoint.online()).await;
    endpoint.close().await;
    match online {
        Ok(()) => Ok(format!("Connected to the premium relay in {} ms.", started.elapsed().as_millis())),
        Err(_) => Err(
            "Could not register with the relay within 15 s. Check the URL, that the server is running, and the licence key."
                .to_string(),
        ),
    }
}

/// Where the "Subscribe" button points, set at build time (`RRPC_PURCHASE_URL`).
pub const DEFAULT_PURCHASE_URL: &str = match option_env!("RRPC_PURCHASE_URL") {
    Some(u) => u,
    None => "",
};

/// Settings kept between runs: `%APPDATA%\rust-remote-pc\settings.txt` (`key=value` lines).
/// The activation code is stored here in plain text; the file is user-private on Windows.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Host uses the premium relay instead of the free public relays.
    pub premium_enabled: bool,
    pub premium_url: String,
    /// The licence key (subscription) or coupon code that unlocks Premium.
    pub premium_license: String,
    /// Where customers buy or renew a subscription.
    pub purchase_url: String,
    /// Quality mode this PC offers to viewers by default.
    pub host_quality: QualityMode,
    /// Quality mode requested when connecting to a PC.
    pub client_quality: QualityMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            premium_enabled: false,
            premium_url: DEFAULT_PREMIUM_URL.to_string(),
            premium_license: String::new(),
            purchase_url: DEFAULT_PURCHASE_URL.to_string(),
            host_quality: QualityMode::Auto,
            client_quality: QualityMode::Auto,
        }
    }
}

/// Whether the entered activation code unlocks Premium right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activation {
    /// Nothing entered.
    NotSet,
    /// Entered but not a valid code.
    Invalid(String),
    /// A subscription that has ended.
    Expired { expired_at: u64 },
    /// Unlocked. `expires_at` is 0 when it never expires.
    Active { kind: Kind, id: String, expires_at: u64 },
}

impl Activation {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// A short line for the UI and logs.
    pub fn describe(&self) -> String {
        match self {
            Self::NotSet => "Not activated".to_string(),
            Self::Invalid(e) => format!("Not valid: {e}"),
            Self::Expired { expired_at } => format!("Subscription expired on {}", format_date(*expired_at)),
            Self::Active { kind: Kind::Coupon, id, .. } => format!("Active: coupon ({id})"),
            Self::Active { id, expires_at: 0, .. } => format!("Active: licence {id}, no end date"),
            Self::Active { id, expires_at, .. } => {
                format!("Active: licence {id}, renews or ends {}", format_date(*expires_at))
            }
        }
    }
}

/// `YYYY-MM-DD` (UTC) for a Unix time.
pub fn format_date(unix: u64) -> String {
    // Days-to-civil conversion (Howard Hinnant's algorithm).
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Opens an `https://` link in the default browser. Anything else is ignored.
pub fn open_link(url: &str) -> bool {
    let url = url.trim();
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    #[cfg(windows)]
    {
        // rundll32 takes the URL as a plain argument; no shell parses it.
        return std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .is_ok();
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(opener).arg(url).spawn().is_ok()
    }
}

impl Settings {
    /// True if a relay URL and an activation code have both been entered.
    pub fn premium_configured(&self) -> bool {
        !self.premium_url.trim().is_empty() && !self.premium_license.trim().is_empty()
    }

    /// Checks the activation code (licence key or coupon) against the clock.
    pub fn activation(&self) -> Activation {
        let code = self.premium_license.trim();
        if code.is_empty() {
            return Activation::NotSet;
        }
        match parse_license(code) {
            Err(e) => Activation::Invalid(e.to_string()),
            Ok(l) => match l.status(unix_now()) {
                Status::Expired { expired_at } => Activation::Expired { expired_at },
                Status::Active { expires_at } => Activation::Active { kind: l.kind, id: l.id, expires_at },
            },
        }
    }

    /// Why Premium cannot be turned on yet, as a message for the user; `None` if it can.
    pub fn premium_blocker(&self) -> Option<String> {
        match self.activation() {
            Activation::Active { .. } => None,
            Activation::NotSet => Some(
                "Premium is a paid feature. Subscribe to get a licence, or enter a coupon code or licence key."
                    .to_string(),
            ),
            Activation::Invalid(e) => Some(format!("The activation code is not valid ({e}).")),
            Activation::Expired { expired_at } => Some(format!(
                "Your subscription ended on {}. Renew it to keep using Premium.",
                format_date(expired_at)
            )),
        }
    }

    /// The host's relay (URL plus a freshly minted pass), if premium is on.
    /// `Ok(None)` means premium is off; `Err` means it is on but not usable.
    pub fn host_relay(&self) -> Result<Option<PremiumRelay>, String> {
        if !self.premium_enabled {
            return Ok(None);
        }
        if let Some(why) = self.premium_blocker() {
            return Err(why);
        }
        let url = self.premium_url.trim();
        if url.is_empty() {
            return Err("Premium is on but no relay URL is set.".to_string());
        }
        let license = parse_license(&self.premium_license).map_err(|e| format!("Activation code is not valid: {e}."))?;
        let relay = PremiumRelay {
            url: url.to_string(),
            key: mint_pass(&license, unix_now() + HOST_PASS_TTL),
        };
        relay.relay_url()?; // validate the URL early
        Ok(Some(relay))
    }

    /// The join code to share for a host with Pairing ID `pairing_id`, if premium is on
    /// and unlocked.
    ///
    /// It carries a pass valid for about a day, so a code that leaks stops working
    /// on its own. The expiry is rounded to the hour so the code stays stable while
    /// it is on screen.
    pub fn join_code(&self, pairing_id: &str) -> Option<String> {
        if !self.premium_enabled || !self.activation().is_active() {
            return None;
        }
        let license = parse_license(&self.premium_license).ok()?;
        let hour = unix_now() / 3600 * 3600;
        Some(
            JoinCode {
                pairing_id: pairing_id.to_string(),
                relay_url: self.premium_url.trim().to_string(),
                pass: mint_pass(&license, hour + 3600 + JOIN_PASS_TTL),
            }
            .encode(),
        )
    }
}

/// True if `s` is a join code from a premium host.
pub fn is_join_code(s: &str) -> bool {
    looks_like_join_code(s)
}

/// Splits a connection target into the Pairing ID to dial and, for a join code, the
/// premium relay (with its pass) to use.
pub fn resolve_target(target: &str) -> Result<(String, Option<PremiumRelay>), String> {
    if looks_like_join_code(target) {
        let jc = JoinCode::decode(target)
            .ok_or_else(|| "That join code is not valid. Copy it again from the PC.".to_string())?;
        return Ok((jc.pairing_id, Some(PremiumRelay { url: jc.relay_url, key: jc.pass })));
    }
    Ok((target.trim().to_string(), None))
}

/// Shortens an address for display; join codes are marked as premium.
pub fn display_target(s: &str) -> String {
    if looks_like_join_code(s) {
        let t = s.trim();
        return format!("Premium join code …{}", &t[t.len().saturating_sub(6)..]);
    }
    display_addr(s)
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.txt"))
}

fn parse_bool(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Loads settings. Environment variables `RRPC_PREMIUM` (on/off), `RRPC_PREMIUM_URL` and
/// `RRPC_PREMIUM_LICENSE` override the file, which is how headless hosts are configured
/// without putting a key on a command line.
pub fn load_settings() -> Settings {
    let mut s = Settings::default();
    if let Some(text) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "premium_enabled" => s.premium_enabled = parse_bool(v),
                "premium_url" if !v.is_empty() => s.premium_url = v.to_string(),
                "premium_license" => s.premium_license = v.to_string(),
                "purchase_url" if !v.is_empty() => s.purchase_url = v.to_string(),
                "host_quality" => s.host_quality = QualityMode::parse(v).unwrap_or_default(),
                "client_quality" => s.client_quality = QualityMode::parse(v).unwrap_or_default(),
                _ => {}
            }
        }
    }
    if let Ok(v) = std::env::var("RRPC_PREMIUM") {
        s.premium_enabled = parse_bool(&v);
    }
    if let Ok(v) = std::env::var("RRPC_PREMIUM_URL") {
        s.premium_url = v;
    }
    if let Ok(v) = std::env::var("RRPC_PREMIUM_LICENSE") {
        s.premium_license = v;
    }
    s
}

pub fn save_settings(s: &Settings) {
    let Some(p) = settings_path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = format!(
        "premium_enabled={}\npremium_url={}\npremium_license={}\npurchase_url={}\nhost_quality={}\nclient_quality={}\n",
        s.premium_enabled,
        s.premium_url.trim(),
        s.premium_license.trim(),
        s.purchase_url.trim(),
        quality_key(s.host_quality),
        quality_key(s.client_quality),
    );
    if let Err(e) = std::fs::write(&p, text) {
        warn!("Could not save settings: {e}");
    }
}

fn quality_key(m: QualityMode) -> &'static str {
    match m {
        QualityMode::Auto => "auto",
        QualityMode::Smooth => "smooth",
        QualityMode::Sharp => "sharp",
        QualityMode::DataSaver => "saver",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_format_correctly() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(951_782_400), "2000-02-29"); // leap day
        assert_eq!(format_date(1_700_000_000), "2023-11-14");
        assert_eq!(format_date(1_795_046_400), "2026-11-19");
        assert_eq!(format_date(1_795_046_399), "2026-11-18");
    }

    #[test]
    fn premium_is_gated_by_activation() {
        let mut s = Settings { premium_enabled: true, premium_url: "https://relay.example".into(), ..Settings::default() };
        // Nothing entered: blocked, and no relay or join code is produced.
        assert_eq!(s.activation(), Activation::NotSet);
        assert!(s.premium_blocker().is_some());
        assert!(s.host_relay().is_err());
        assert!(s.join_code(&"ab".repeat(32)).is_none());
        // Garbage: blocked.
        s.premium_license = "not-a-code".into();
        assert!(matches!(s.activation(), Activation::Invalid(_)));
        assert!(s.host_relay().is_err());
        // Premium off: no relay needed, no error.
        s.premium_enabled = false;
        assert_eq!(s.host_relay(), Ok(None));
    }

    #[test]
    fn coupon_and_licence_unlock_premium() {
        let secret = "unit-test-secret-0123456789";
        let mut s = Settings { premium_enabled: true, premium_url: "https://relay.example".into(), ..Settings::default() };

        s.premium_license = premium_auth::issue_coupon(secret, "owner").unwrap();
        assert!(s.activation().is_active());
        assert!(s.host_relay().unwrap().is_some());
        let code = s.join_code(&"ab".repeat(32)).unwrap();
        assert!(JoinCode::decode(&code).is_some());

        s.premium_license = premium_auth::issue_license(secret, "acme", unix_now() + 86_400).unwrap();
        assert!(s.activation().is_active());

        // An ended subscription is blocked, with a renewal message.
        s.premium_license = premium_auth::issue_license(secret, "acme", unix_now() - 10).unwrap();
        assert!(matches!(s.activation(), Activation::Expired { .. }));
        assert!(s.premium_blocker().unwrap().contains("Renew"));
        assert!(s.host_relay().is_err());
        assert!(s.join_code(&"ab".repeat(32)).is_none());
    }

    #[test]
    fn only_https_links_open() {
        assert!(!open_link("http://example.com"));
        assert!(!open_link("javascript:alert(1)"));
        assert!(!open_link("https://exa mple.com"));
        assert!(!open_link(""));
    }
}
