//! Licence keys, relay passes and join codes for the premium relay.
//!
//! # Model
//!
//! * The relay operator holds a **server secret** `S`.
//! * A premium customer gets a **licence key**: `rrpc-lic1.<id>.<K>` where
//!   `K = HMAC-SHA256(S, "license|" + id)`. The customer's host app holds `K`.
//! * The host app uses `K` to mint **passes**: `<id>.<expiry>.<mac>` where
//!   `mac = HMAC-SHA256(K, "pass|" + id + "|" + expiry)` (first 16 bytes).
//!   A pass is the bearer token presented to the relay.
//! * The relay recomputes `K` from `S` and the id in the pass, checks the mac and
//!   the expiry, and checks the id is not revoked. No database is needed.
//! * The host puts a short-lived pass in the **join code** it shares, so clients
//!   can use the relay without ever seeing the licence key.
//!
//! A leaked pass stops working at its expiry; a leaked licence can be revoked by
//! listing its id on the relay.

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

const LICENSE_PREFIX: &str = "rrpc-lic1";
const JOIN_PREFIX: &str = "rrpc1.";
const PASS_MAC_LEN: usize = 16;

/// A pass is rejected if it expires further in the future than this, so a
/// mistake when minting cannot create an effectively permanent credential.
pub const MAX_PASS_TTL_SECS: u64 = 60 * 24 * 3600;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    BadId,
    BadFormat,
    BadSignature,
    Expired,
    TooLong,
    Revoked,
    SecretTooShort,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadId => "licence id must be 1-32 characters of a-z, 0-9 or '-'",
            Self::BadFormat => "malformed key or pass",
            Self::BadSignature => "signature does not match",
            Self::Expired => "pass has expired",
            Self::TooLong => "pass expires too far in the future",
            Self::Revoked => "licence has been revoked",
            Self::SecretTooShort => "server secret must be at least 16 characters",
        })
    }
}

impl std::error::Error for AuthError {}

/// A parsed licence key (held by the host app).
#[derive(Clone, PartialEq, Eq)]
pub struct License {
    pub id: String,
    key: [u8; 32],
}

impl fmt::Debug for License {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("License").field("id", &self.id).field("key", &"<hidden>").finish()
    }
}

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn mac(key: &[u8], msg: &[u8]) -> HmacSha256 {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(msg);
    m
}

fn derive_key(secret: &str, id: &str) -> [u8; 32] {
    mac(secret.as_bytes(), format!("license|{id}").as_bytes()).finalize().into_bytes().into()
}

/// Operator side: issues the licence key string for customer `id`.
pub fn issue_license(secret: &str, id: &str) -> Result<String, AuthError> {
    if secret.len() < 16 {
        return Err(AuthError::SecretTooShort);
    }
    if !is_valid_id(id) {
        return Err(AuthError::BadId);
    }
    let k = derive_key(secret, id);
    Ok(format!("{LICENSE_PREFIX}.{id}.{}", BASE64URL_NOPAD.encode(&k)))
}

/// Host side: parses a licence key string.
pub fn parse_license(s: &str) -> Result<License, AuthError> {
    let mut parts = s.trim().split('.');
    let (Some(prefix), Some(id), Some(k), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AuthError::BadFormat);
    };
    if prefix != LICENSE_PREFIX {
        return Err(AuthError::BadFormat);
    }
    if !is_valid_id(id) {
        return Err(AuthError::BadId);
    }
    let bytes = BASE64URL_NOPAD.decode(k.as_bytes()).map_err(|_| AuthError::BadFormat)?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| AuthError::BadFormat)?;
    Ok(License { id: id.to_string(), key })
}

fn pass_mac(key: &[u8], id: &str, expires: u64) -> [u8; PASS_MAC_LEN] {
    let full = mac(key, format!("pass|{id}|{expires}").as_bytes()).finalize().into_bytes();
    let mut out = [0u8; PASS_MAC_LEN];
    out.copy_from_slice(&full[..PASS_MAC_LEN]);
    out
}

/// Host side: mints a pass valid until `expires_at` (seconds since the Unix epoch).
pub fn mint_pass(license: &License, expires_at: u64) -> String {
    let m = pass_mac(&license.key, &license.id, expires_at);
    format!("{}.{}.{}", license.id, expires_at, BASE64URL_NOPAD.encode(&m))
}

/// Relay side: checks a pass and returns the licence id it belongs to.
///
/// `is_revoked` is asked about the licence id once the signature checks out.
pub fn verify_pass(
    secret: &str,
    pass: &str,
    now: u64,
    is_revoked: impl Fn(&str) -> bool,
) -> Result<String, AuthError> {
    let mut parts = pass.trim().split('.');
    let (Some(id), Some(exp), Some(m), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AuthError::BadFormat);
    };
    if !is_valid_id(id) {
        return Err(AuthError::BadId);
    }
    let expires: u64 = exp.parse().map_err(|_| AuthError::BadFormat)?;
    let given = BASE64URL_NOPAD.decode(m.as_bytes()).map_err(|_| AuthError::BadFormat)?;

    let key = derive_key(secret, id);
    let expected = pass_mac(&key, id, expires);
    // Constant-time comparison.
    let diff = expected
        .iter()
        .zip(given.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        | (expected.len() ^ given.len()) as u8;
    if diff != 0 {
        return Err(AuthError::BadSignature);
    }
    if expires < now {
        return Err(AuthError::Expired);
    }
    if expires - now > MAX_PASS_TTL_SECS {
        return Err(AuthError::TooLong);
    }
    if is_revoked(id) {
        return Err(AuthError::Revoked);
    }
    Ok(id.to_string())
}

/// What a host shares so a client can connect: the host's Pairing ID, the relay
/// to meet on, and a short-lived pass for that relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinCode {
    pub pairing_id: String,
    pub relay_url: String,
    pub pass: String,
}

impl JoinCode {
    pub fn encode(&self) -> String {
        let raw = format!("{}|{}|{}", self.pairing_id, self.relay_url, self.pass);
        format!("{JOIN_PREFIX}{}", BASE64URL_NOPAD.encode(raw.as_bytes()))
    }

    /// Returns `None` if `s` is not a join code. Surrounding whitespace is ignored.
    pub fn decode(s: &str) -> Option<Self> {
        let body = s.trim().strip_prefix(JOIN_PREFIX)?;
        let raw = String::from_utf8(BASE64URL_NOPAD.decode(body.as_bytes()).ok()?).ok()?;
        let mut parts = raw.split('|');
        let (id, url, pass, none) = (parts.next()?, parts.next()?, parts.next()?, parts.next());
        if none.is_some() || id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        if url.is_empty() || pass.is_empty() {
            return None;
        }
        Some(Self { pairing_id: id.to_ascii_lowercase(), relay_url: url.to_string(), pass: pass.to_string() })
    }
}

pub fn looks_like_join_code(s: &str) -> bool {
    s.trim().starts_with(JOIN_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "unit-test-secret-0123456789";
    const NOW: u64 = 1_800_000_000;

    fn license() -> License {
        parse_license(&issue_license(SECRET, "acme-1").unwrap()).unwrap()
    }

    #[test]
    fn pass_round_trip() {
        let pass = mint_pass(&license(), NOW + 3600);
        assert_eq!(verify_pass(SECRET, &pass, NOW, |_| false), Ok("acme-1".to_string()));
    }

    #[test]
    fn wrong_secret_rejected() {
        let pass = mint_pass(&license(), NOW + 3600);
        assert_eq!(
            verify_pass("another-secret-0123456789", &pass, NOW, |_| false),
            Err(AuthError::BadSignature)
        );
    }

    #[test]
    fn tampered_expiry_rejected() {
        let pass = mint_pass(&license(), NOW + 3600);
        let forged = pass.replacen(&format!("{}", NOW + 3600), &format!("{}", NOW + 7200), 1);
        assert_eq!(verify_pass(SECRET, &forged, NOW, |_| false), Err(AuthError::BadSignature));
    }

    #[test]
    fn tampered_id_rejected() {
        let pass = mint_pass(&license(), NOW + 3600);
        let forged = pass.replacen("acme-1", "acme-2", 1);
        assert_eq!(verify_pass(SECRET, &forged, NOW, |_| false), Err(AuthError::BadSignature));
    }

    #[test]
    fn expired_rejected() {
        let pass = mint_pass(&license(), NOW - 1);
        assert_eq!(verify_pass(SECRET, &pass, NOW, |_| false), Err(AuthError::Expired));
    }

    #[test]
    fn far_future_rejected() {
        let pass = mint_pass(&license(), NOW + MAX_PASS_TTL_SECS + 10);
        assert_eq!(verify_pass(SECRET, &pass, NOW, |_| false), Err(AuthError::TooLong));
    }

    #[test]
    fn revoked_rejected() {
        let pass = mint_pass(&license(), NOW + 3600);
        assert_eq!(verify_pass(SECRET, &pass, NOW, |id| id == "acme-1"), Err(AuthError::Revoked));
    }

    #[test]
    fn garbage_rejected() {
        for bad in ["", "x", "a.b", "a.1.c.d", "acme-1.notanumber.AAAA", "..", "ACME.1.AAAA"] {
            assert!(verify_pass(SECRET, bad, NOW, |_| false).is_err(), "{bad}");
        }
    }

    #[test]
    fn license_validation() {
        assert_eq!(issue_license("short", "acme"), Err(AuthError::SecretTooShort));
        assert_eq!(issue_license(SECRET, "Bad Id"), Err(AuthError::BadId));
        assert!(parse_license("rrpc-lic1.acme.@@@").is_err());
        assert!(parse_license("other.acme.AAAA").is_err());
    }

    #[test]
    fn join_code_round_trip() {
        let jc = JoinCode {
            pairing_id: "ab".repeat(32),
            relay_url: "https://relay.example.onrender.com".into(),
            pass: mint_pass(&license(), NOW + 3600),
        };
        let s = jc.encode();
        assert!(s.starts_with("rrpc1."));
        assert_eq!(JoinCode::decode(&format!("  {s}\n")), Some(jc));
    }

    #[test]
    fn join_code_rejects_junk() {
        assert!(JoinCode::decode("nope").is_none());
        assert!(JoinCode::decode("rrpc1.!!!").is_none());
        assert!(JoinCode::decode(&format!("rrpc1.{}", BASE64URL_NOPAD.encode(b"a|b|c"))).is_none());
    }
}
