//! Activation codes, relay passes and join codes for the premium relay.
//!
//! # Model
//!
//! * The relay operator holds a **server secret** `S`.
//! * A premium host holds an **activation code**, which is one of:
//!   * a **licence key** (sold as a subscription): `rrpc-lic2.<id>.<expiry>.<K>`
//!     with `K = HMAC-SHA256(S, "license|" id "|" expiry)`; `expiry` is a Unix time
//!     (0 means it never expires). The older `rrpc-lic1.<id>.<K>` form is still accepted.
//!   * a **coupon** (free, for the owner): `RRPC-<ID>-XXXX-XXXX-XXXX-XXXX`. The four
//!     groups are the first 80 bits of `HMAC-SHA256(S, "coupon|" id)`.
//! * The host derives a signing key from its activation code and uses it to mint
//!   **passes**, the bearer tokens presented to the relay:
//!   `<id>.<licence-expiry|c>.<pass-expiry>.<mac>`.
//! * The relay recomputes the same key from `S` and the id in the pass, so it needs no
//!   database. It checks the signature, both expiries and a revocation list.
//! * The host puts a short-lived pass in the **join code** it shares, so clients can use
//!   the relay without ever seeing the activation code.
//!
//! A leaked pass stops working at its expiry. A leaked licence or coupon can be revoked by
//! listing its id on the relay.

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

const LICENSE_PREFIX_V1: &str = "rrpc-lic1";
const LICENSE_PREFIX_V2: &str = "rrpc-lic2";
const COUPON_PREFIX: &str = "RRPC-";
const JOIN_PREFIX: &str = "rrpc1."; // older invites without a password
const INVITE_PREFIX: &str = "rrpc2.";
const PASS_MAC_LEN: usize = 16;
/// Marker in a pass for "signed with a coupon-derived key".
const COUPON_MARK: &str = "c";
/// Crockford base32: no I, L, O or U, so codes survive being read aloud or typed.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const COUPON_BYTES: usize = 10;
const COUPON_CODE_LEN: usize = 19; // 4 groups of 4 plus 3 hyphens

/// A pass is rejected if it expires further in the future than this, so a
/// mistake when minting cannot create an effectively permanent credential.
pub const MAX_PASS_TTL_SECS: u64 = 60 * 24 * 3600;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    BadId,
    BadFormat,
    BadSignature,
    Expired,
    LicenseExpired,
    TooLong,
    Revoked,
    SecretTooShort,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadId => "id must be 1-32 characters of a-z, 0-9 or '-'",
            Self::BadFormat => "malformed code or pass",
            Self::BadSignature => "signature does not match",
            Self::Expired => "pass has expired",
            Self::LicenseExpired => "subscription has expired",
            Self::TooLong => "pass expires too far in the future",
            Self::Revoked => "licence has been revoked",
            Self::SecretTooShort => "server secret must be at least 16 characters",
        })
    }
}

impl std::error::Error for AuthError {}

/// What kind of activation code a [`License`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A paid licence key, normally with an expiry (subscription).
    Licence,
    /// A free coupon (never expires).
    Coupon,
}

/// A parsed activation code (held by the host app).
#[derive(Clone, PartialEq, Eq)]
pub struct License {
    pub id: String,
    pub kind: Kind,
    /// Unix time the licence ends; 0 means it never expires.
    pub expires_at: u64,
    key: [u8; 32],
    /// True for the legacy `rrpc-lic1` form, which signs passes in the older layout.
    legacy: bool,
}

impl fmt::Debug for License {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("License")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("expires_at", &self.expires_at)
            .field("key", &"<hidden>")
            .finish()
    }
}

/// Whether an activation code can be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Usable; `expires_at` is 0 if it never expires.
    Active { expires_at: u64 },
    Expired { expired_at: u64 },
}

impl License {
    pub fn status(&self, now: u64) -> Status {
        if self.expires_at != 0 && self.expires_at < now {
            Status::Expired { expired_at: self.expires_at }
        } else {
            Status::Active { expires_at: self.expires_at }
        }
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

fn check_secret(secret: &str) -> Result<(), AuthError> {
    if secret.len() < 16 {
        Err(AuthError::SecretTooShort)
    } else {
        Ok(())
    }
}

// ── Licence keys ────────────────────────────────────────────────────────────

fn licence_key_v1(secret: &str, id: &str) -> [u8; 32] {
    mac(secret.as_bytes(), format!("license|{id}").as_bytes()).finalize().into_bytes().into()
}

fn licence_key_v2(secret: &str, id: &str, expires_at: u64) -> [u8; 32] {
    mac(secret.as_bytes(), format!("license|{id}|{expires_at}").as_bytes())
        .finalize()
        .into_bytes()
        .into()
}

/// Operator side: issues a licence key for customer `id`, valid until `expires_at`
/// (Unix seconds; 0 for no expiry).
pub fn issue_license(secret: &str, id: &str, expires_at: u64) -> Result<String, AuthError> {
    check_secret(secret)?;
    if !is_valid_id(id) {
        return Err(AuthError::BadId);
    }
    let k = licence_key_v2(secret, id, expires_at);
    Ok(format!("{LICENSE_PREFIX_V2}.{id}.{expires_at}.{}", BASE64URL_NOPAD.encode(&k)))
}

fn parse_licence_key(s: &str) -> Result<License, AuthError> {
    let parts: Vec<&str> = s.split('.').collect();
    let (prefix, id, expires_at, k) = match parts.as_slice() {
        [p, id, k] if *p == LICENSE_PREFIX_V1 => (*p, *id, 0u64, *k),
        [p, id, exp, k] if *p == LICENSE_PREFIX_V2 => {
            (*p, *id, exp.parse::<u64>().map_err(|_| AuthError::BadFormat)?, *k)
        }
        _ => return Err(AuthError::BadFormat),
    };
    if !is_valid_id(id) {
        return Err(AuthError::BadId);
    }
    let bytes = BASE64URL_NOPAD.decode(k.as_bytes()).map_err(|_| AuthError::BadFormat)?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| AuthError::BadFormat)?;
    Ok(License {
        id: id.to_string(),
        kind: Kind::Licence,
        expires_at,
        key,
        legacy: prefix == LICENSE_PREFIX_V1,
    })
}

// ── Coupons ─────────────────────────────────────────────────────────────────

fn coupon_secret_bytes(secret: &str, id: &str) -> [u8; COUPON_BYTES] {
    let full = mac(secret.as_bytes(), format!("coupon|{id}").as_bytes()).finalize().into_bytes();
    let mut out = [0u8; COUPON_BYTES];
    out.copy_from_slice(&full[..COUPON_BYTES]);
    out
}

fn coupon_signing_key(id: &str, code: &[u8; COUPON_BYTES]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"rrpc-coupon-key|");
    h.update(id.as_bytes());
    h.update(b"|");
    h.update(code);
    h.finalize().into()
}

fn crockford_encode(bytes: &[u8; COUPON_BYTES]) -> String {
    let mut out = String::with_capacity(16);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in bytes {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(CROCKFORD[((acc >> bits) & 31) as usize] as char);
        }
    }
    out
}

fn crockford_decode(s: &str) -> Option<[u8; COUPON_BYTES]> {
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut out = Vec::with_capacity(COUPON_BYTES);
    for c in s.chars() {
        // Crockford's tolerance for look-alike characters.
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        let v = CROCKFORD.iter().position(|&x| x as char == c)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    out.try_into().ok()
}

/// Operator side: issues a free coupon for `id` (for example `owner`). Coupons never expire.
pub fn issue_coupon(secret: &str, id: &str) -> Result<String, AuthError> {
    check_secret(secret)?;
    if !is_valid_id(id) {
        return Err(AuthError::BadId);
    }
    let enc = crockford_encode(&coupon_secret_bytes(secret, id));
    let groups: Vec<&str> = (0..4).map(|i| &enc[i * 4..i * 4 + 4]).collect();
    Ok(format!("{COUPON_PREFIX}{}-{}", id.to_ascii_uppercase(), groups.join("-")))
}

fn parse_coupon(s: &str) -> Result<License, AuthError> {
    let upper = s.trim().to_ascii_uppercase();
    let rest = upper.strip_prefix(COUPON_PREFIX).ok_or(AuthError::BadFormat)?;
    if rest.len() <= COUPON_CODE_LEN + 1 {
        return Err(AuthError::BadFormat);
    }
    let split = rest.len() - COUPON_CODE_LEN;
    let (id_part, code_part) = rest.split_at(split);
    let id = id_part.strip_suffix('-').ok_or(AuthError::BadFormat)?.to_ascii_lowercase();
    if !is_valid_id(&id) {
        return Err(AuthError::BadId);
    }
    let code = crockford_decode(&code_part.replace('-', "")).ok_or(AuthError::BadFormat)?;
    Ok(License {
        key: coupon_signing_key(&id, &code),
        id,
        kind: Kind::Coupon,
        expires_at: 0,
        legacy: false,
    })
}

// ── Parsing any activation code ─────────────────────────────────────────────

/// Host side: parses a licence key or a coupon.
pub fn parse_license(s: &str) -> Result<License, AuthError> {
    let s = s.trim();
    // Licence keys contain dots and coupons never do, so the two cannot be confused even
    // though both start with "rrpc-".
    if !s.contains('.') && s.to_ascii_uppercase().starts_with(COUPON_PREFIX) {
        parse_coupon(s)
    } else {
        parse_licence_key(s)
    }
}

// ── Passes ──────────────────────────────────────────────────────────────────

fn pass_mac(key: &[u8], msg: &str) -> [u8; PASS_MAC_LEN] {
    let full = mac(key, msg.as_bytes()).finalize().into_bytes();
    let mut out = [0u8; PASS_MAC_LEN];
    out.copy_from_slice(&full[..PASS_MAC_LEN]);
    out
}

/// Host side: mints a pass valid until `expires_at` (seconds since the Unix epoch),
/// never later than the licence itself.
pub fn mint_pass(license: &License, expires_at: u64) -> String {
    let exp = if license.expires_at != 0 { expires_at.min(license.expires_at) } else { expires_at };
    if license.legacy {
        let m = pass_mac(&license.key, &format!("pass|{}|{exp}", license.id));
        return format!("{}.{exp}.{}", license.id, BASE64URL_NOPAD.encode(&m));
    }
    let mark = if license.kind == Kind::Coupon { COUPON_MARK.to_string() } else { license.expires_at.to_string() };
    let m = pass_mac(&license.key, &format!("pass|{}|{mark}|{exp}", license.id));
    format!("{}.{mark}.{exp}.{}", license.id, BASE64URL_NOPAD.encode(&m))
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
    let parts: Vec<&str> = pass.trim().split('.').collect();
    // (id, licence marker or None for legacy, pass expiry, mac, signing key, message)
    let (id, expires, given, key, msg, licence_expiry) = match parts.as_slice() {
        // Legacy layout: id.exp.mac
        [id, exp, m] => {
            if !is_valid_id(id) {
                return Err(AuthError::BadId);
            }
            let e: u64 = exp.parse().map_err(|_| AuthError::BadFormat)?;
            (*id, e, *m, licence_key_v1(secret, id), format!("pass|{id}|{e}"), 0u64)
        }
        // Current layout: id.licence-expiry|c.exp.mac
        [id, mark, exp, m] => {
            if !is_valid_id(id) {
                return Err(AuthError::BadId);
            }
            let e: u64 = exp.parse().map_err(|_| AuthError::BadFormat)?;
            if *mark == COUPON_MARK {
                let code = coupon_secret_bytes(secret, id);
                (*id, e, *m, coupon_signing_key(id, &code), format!("pass|{id}|{mark}|{e}"), 0)
            } else {
                let le: u64 = mark.parse().map_err(|_| AuthError::BadFormat)?;
                (*id, e, *m, licence_key_v2(secret, id, le), format!("pass|{id}|{mark}|{e}"), le)
            }
        }
        _ => return Err(AuthError::BadFormat),
    };
    let given = BASE64URL_NOPAD.decode(given.as_bytes()).map_err(|_| AuthError::BadFormat)?;
    if !ct_eq(&pass_mac(&key, &msg), &given) {
        return Err(AuthError::BadSignature);
    }
    if licence_expiry != 0 && licence_expiry < now {
        return Err(AuthError::LicenseExpired);
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

// ── Invites ─────────────────────────────────────────────────────────────────

/// Everything a client needs to connect, as one string a host can copy and send.
///
/// * `pairing_id`: which PC to reach.
/// * `password`: the host's password, so the client does not ask for it separately.
/// * `relay_url` / `pass`: for a premium host, the relay to meet on and a short-lived
///   pass for it. Both empty for a standard host.
///
/// Anyone holding an invite can control the PC while it is shared, so treat it like a
/// password. Invites from older versions (`rrpc1.`, no password) are still understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub pairing_id: String,
    pub password: String,
    pub relay_url: String,
    pub pass: String,
}

impl Invite {
    pub fn has_relay(&self) -> bool {
        !self.relay_url.is_empty()
    }

    pub fn encode(&self) -> String {
        // Newline-separated: a single-line password can never contain one.
        let raw = format!("{}\n{}\n{}\n{}", self.pairing_id, self.password, self.relay_url, self.pass);
        format!("{INVITE_PREFIX}{}", BASE64URL_NOPAD.encode(raw.as_bytes()))
    }

    /// Returns `None` if `s` is not an invite. Surrounding whitespace is ignored.
    pub fn decode(s: &str) -> Option<Self> {
        let s = s.trim();
        let (body, legacy) = match s.strip_prefix(INVITE_PREFIX) {
            Some(b) => (b, false),
            None => (s.strip_prefix(JOIN_PREFIX)?, true),
        };
        let raw = String::from_utf8(BASE64URL_NOPAD.decode(body.as_bytes()).ok()?).ok()?;
        let (id, password, relay_url, pass) = if legacy {
            // rrpc1: id|relay|pass
            let mut p = raw.split('|');
            let (id, url, pass, extra) = (p.next()?, p.next()?, p.next()?, p.next());
            if extra.is_some() {
                return None;
            }
            (id, "", url, pass)
        } else {
            let mut p = raw.split('\n');
            let (id, pw, url, pass, extra) = (p.next()?, p.next()?, p.next()?, p.next()?, p.next());
            if extra.is_some() {
                return None;
            }
            (id, pw, url, pass)
        };
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        // A relay URL and its pass go together.
        if relay_url.is_empty() != pass.is_empty() {
            return None;
        }
        Some(Self {
            pairing_id: id.to_ascii_lowercase(),
            password: password.to_string(),
            relay_url: relay_url.to_string(),
            pass: pass.to_string(),
        })
    }
}

/// True if `s` looks like an invite (current or older format).
pub fn looks_like_invite(s: &str) -> bool {
    let t = s.trim();
    t.starts_with(INVITE_PREFIX) || t.starts_with(JOIN_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "unit-test-secret-0123456789";
    const NOW: u64 = 1_800_000_000;

    fn licence(expires_at: u64) -> License {
        parse_license(&issue_license(SECRET, "acme-1", expires_at).unwrap()).unwrap()
    }

    fn coupon() -> License {
        parse_license(&issue_coupon(SECRET, "owner").unwrap()).unwrap()
    }

    fn ok(secret: &str, pass: &str, now: u64) -> Result<String, AuthError> {
        verify_pass(secret, pass, now, |_| false)
    }

    #[test]
    fn licence_pass_round_trip() {
        let pass = mint_pass(&licence(NOW + 86_400), NOW + 3600);
        assert_eq!(ok(SECRET, &pass, NOW), Ok("acme-1".to_string()));
    }

    #[test]
    fn licence_without_expiry_round_trip() {
        let pass = mint_pass(&licence(0), NOW + 3600);
        assert_eq!(ok(SECRET, &pass, NOW), Ok("acme-1".to_string()));
    }

    #[test]
    fn coupon_round_trip() {
        let c = coupon();
        assert_eq!((c.kind, c.expires_at), (Kind::Coupon, 0));
        let pass = mint_pass(&c, NOW + 3600);
        assert_eq!(ok(SECRET, &pass, NOW), Ok("owner".to_string()));
    }

    #[test]
    fn coupon_format_and_typo_tolerance() {
        let code = issue_coupon(SECRET, "owner").unwrap();
        assert!(code.starts_with("RRPC-OWNER-"));
        assert_eq!(code.len(), "RRPC-OWNER-".len() + COUPON_CODE_LEN);
        // lower case and spaces around are fine
        let a = parse_license(&format!("  {}  ", code.to_lowercase())).unwrap();
        assert_eq!(a.id, "owner");
        let pass = mint_pass(&a, NOW + 60);
        assert!(ok(SECRET, &pass, NOW).is_ok());
    }

    #[test]
    fn coupon_with_hyphenated_id() {
        let code = issue_coupon(SECRET, "beta-corp").unwrap();
        let l = parse_license(&code).unwrap();
        assert_eq!(l.id, "beta-corp");
        assert!(ok(SECRET, &mint_pass(&l, NOW + 60), NOW).is_ok());
    }

    #[test]
    fn wrong_coupon_rejected() {
        let mut code = issue_coupon(SECRET, "owner").unwrap();
        // change the last character to a different valid one
        let last = code.pop().unwrap();
        code.push(if last == '2' { '3' } else { '2' });
        let l = parse_license(&code).unwrap();
        assert_eq!(ok(SECRET, &mint_pass(&l, NOW + 60), NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn coupon_for_other_secret_rejected() {
        let pass = mint_pass(&coupon(), NOW + 60);
        assert_eq!(ok("another-secret-0123456789", &pass, NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn coupon_cannot_be_forged_from_id_alone() {
        // Someone who knows the id but not the code cannot sign a valid pass.
        let forged = format!("owner.c.{}.AAAAAAAAAAAAAAAAAAAAAA", NOW + 60);
        assert_eq!(ok(SECRET, &forged, NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn expired_subscription_rejected_by_server() {
        let l = licence(NOW - 10);
        assert_eq!(l.status(NOW), Status::Expired { expired_at: NOW - 10 });
        // The host would not mint it, but a stored pass must still be refused.
        let pass = {
            // mint with a pass expiry in the future, ignoring the cap by building it at an earlier time
            let early = NOW - 3600;
            mint_pass(&licence(NOW - 10), early + 3000)
        };
        assert!(matches!(ok(SECRET, &pass, NOW), Err(AuthError::LicenseExpired) | Err(AuthError::Expired)));
    }

    #[test]
    fn pass_never_outlives_licence() {
        let l = licence(NOW + 100);
        let pass = mint_pass(&l, NOW + 10_000);
        let exp: u64 = pass.split('.').nth(2).unwrap().parse().unwrap();
        assert_eq!(exp, NOW + 100);
    }

    #[test]
    fn licence_status() {
        assert_eq!(licence(0).status(NOW), Status::Active { expires_at: 0 });
        assert_eq!(licence(NOW + 5).status(NOW), Status::Active { expires_at: NOW + 5 });
        assert_eq!(coupon().status(NOW), Status::Active { expires_at: 0 });
    }

    #[test]
    fn tampered_licence_expiry_rejected() {
        // A customer cannot extend their subscription by editing the expiry in a pass.
        let pass = mint_pass(&licence(NOW + 100), NOW + 50);
        let forged = pass.replacen(&format!(".{}.", NOW + 100), &format!(".{}.", NOW + 999_999), 1);
        assert_eq!(ok(SECRET, &forged, NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn legacy_v1_licence_still_works() {
        let k = licence_key_v1(SECRET, "old-1");
        let s = format!("{LICENSE_PREFIX_V1}.old-1.{}", BASE64URL_NOPAD.encode(&k));
        let l = parse_license(&s).unwrap();
        let pass = mint_pass(&l, NOW + 3600);
        assert_eq!(pass.split('.').count(), 3);
        assert_eq!(ok(SECRET, &pass, NOW), Ok("old-1".to_string()));
    }

    #[test]
    fn wrong_secret_rejected() {
        let pass = mint_pass(&licence(0), NOW + 3600);
        assert_eq!(ok("another-secret-0123456789", &pass, NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn tampered_id_rejected() {
        let pass = mint_pass(&licence(0), NOW + 3600);
        let forged = pass.replacen("acme-1", "acme-2", 1);
        assert_eq!(ok(SECRET, &forged, NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn expired_pass_rejected() {
        let pass = mint_pass(&licence(0), NOW - 1);
        assert_eq!(ok(SECRET, &pass, NOW), Err(AuthError::Expired));
    }

    #[test]
    fn far_future_rejected() {
        let pass = mint_pass(&licence(0), NOW + MAX_PASS_TTL_SECS + 10);
        assert_eq!(ok(SECRET, &pass, NOW), Err(AuthError::TooLong));
    }

    #[test]
    fn revoked_rejected() {
        for l in [licence(0), coupon()] {
            let pass = mint_pass(&l, NOW + 3600);
            let id = l.id.clone();
            assert_eq!(verify_pass(SECRET, &pass, NOW, |x| x == id), Err(AuthError::Revoked));
        }
    }

    #[test]
    fn garbage_rejected() {
        for bad in ["", "x", "a.b", "a.1.c.d.e", "acme-1.notanumber.AAAA", "..", "ACME.1.AAAA", "owner.c.x.AAAA"] {
            assert!(ok(SECRET, bad, NOW).is_err(), "{bad}");
        }
    }

    #[test]
    fn code_validation() {
        assert_eq!(issue_license("short", "acme", 0), Err(AuthError::SecretTooShort));
        assert_eq!(issue_license(SECRET, "Bad Id", 0), Err(AuthError::BadId));
        assert_eq!(issue_coupon("short", "owner"), Err(AuthError::SecretTooShort));
        assert!(parse_license("rrpc-lic2.acme.0.@@@").is_err());
        assert!(parse_license("other.acme.AAAA").is_err());
        assert!(parse_license("RRPC-OWNER-1234").is_err());
        assert!(parse_license("RRPC-OWNER-UUUU-UUUU-UUUU-UUUU").is_err()); // U is not in the alphabet
    }

    fn standard_invite() -> Invite {
        Invite {
            pairing_id: "ab".repeat(32),
            password: "pw-with|odd chars & 'quotes'".into(),
            relay_url: String::new(),
            pass: String::new(),
        }
    }

    #[test]
    fn standard_invite_round_trip() {
        let inv = standard_invite();
        let s = inv.encode();
        assert!(s.starts_with("rrpc2."));
        assert!(!inv.has_relay());
        assert_eq!(Invite::decode(&format!("  {s}\n")), Some(inv));
    }

    #[test]
    fn premium_invite_round_trip() {
        let inv = Invite {
            relay_url: "https://relay.example.onrender.com".into(),
            pass: mint_pass(&coupon(), NOW + 3600),
            ..standard_invite()
        };
        assert!(inv.has_relay());
        assert_eq!(Invite::decode(&inv.encode()), Some(inv));
    }

    #[test]
    fn empty_password_is_allowed() {
        let inv = Invite { password: String::new(), ..standard_invite() };
        assert_eq!(Invite::decode(&inv.encode()), Some(inv));
    }

    #[test]
    fn older_rrpc1_invites_still_decode() {
        let raw = format!("{}|https://relay.example|somepass", "ab".repeat(32));
        let s = format!("rrpc1.{}", BASE64URL_NOPAD.encode(raw.as_bytes()));
        let inv = Invite::decode(&s).unwrap();
        assert_eq!(inv.password, "");
        assert_eq!(inv.relay_url, "https://relay.example");
        assert_eq!(inv.pass, "somepass");
        assert!(looks_like_invite(&s));
    }

    #[test]
    fn invite_rejects_junk() {
        assert!(Invite::decode("nope").is_none());
        assert!(Invite::decode("rrpc2.!!!").is_none());
        let enc = |raw: &str| format!("rrpc2.{}", BASE64URL_NOPAD.encode(raw.as_bytes()));
        assert!(Invite::decode(&enc("a\nb\nc\nd")).is_none()); // id is not 64 hex chars
        let id = "ab".repeat(32);
        assert!(Invite::decode(&enc(&format!("{id}\npw\nhttps://relay\n"))).is_none()); // relay without a pass
        assert!(Invite::decode(&enc(&format!("{id}\npw\n\npass"))).is_none()); // pass without a relay
        assert!(Invite::decode(&enc(&format!("{id}\npw\n\n\nextra"))).is_none()); // too many fields
        assert!(!looks_like_invite("b6a1fada41"));
    }
}
