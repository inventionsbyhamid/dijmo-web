//! Secrets and tokens.
//!
//! `SECRET_KEY` never signs anything directly: purpose-specific subkeys are
//! derived from it (HMAC-SHA256 over a label), so one use can never be
//! replayed as another.
//!
//! - **Confirmation tokens:** 32 random bytes, base64url. Only their SHA-256 is stored.
//! - **Leave tokens:** the signup id plus an HMAC of it, so every later email
//!   can carry an unsubscribe link without storing a token.
//! - **IP hashes:** HMAC of the IP, never the IP itself.

use std::fmt;
use std::net::IpAddr;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// The decoded `SECRET_KEY`. Its `Debug` output is redacted.
#[derive(Clone)]
pub struct SecretKey(Vec<u8>);

impl SecretKey {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(<redacted>)")
    }
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().into()
}

/// Purpose-specific keys derived from `SECRET_KEY`.
#[derive(Clone)]
pub struct Keys {
    ip: [u8; 32],
    leave: [u8; 32],
}

impl fmt::Debug for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Keys(<redacted>)")
    }
}

/// Length of a leave tag (truncated HMAC).
const LEAVE_TAG_LEN: usize = 16;

impl Keys {
    pub fn derive(secret: &SecretKey) -> Self {
        Self {
            ip: hmac(&secret.0, &[b"dijmo/ip-hash/v1"]),
            leave: hmac(&secret.0, &[b"dijmo/waitlist-leave/v1"]),
        }
    }

    /// The coarse IP hash stored with a signup: IPv4 /24, IPv6 /48.
    pub fn coarse_ip_hash(&self, ip: IpAddr) -> Vec<u8> {
        let prefix = match canonical(ip) {
            IpAddr::V4(v4) => {
                let o = v4.octets();
                vec![4, o[0], o[1], o[2]]
            }
            IpAddr::V6(v6) => {
                let mut v = vec![6];
                v.extend_from_slice(&v6.octets()[..6]);
                v
            }
        };
        hmac(&self.ip, &[b"coarse", &prefix])[..16].to_vec()
    }

    /// The rate-limit key for an IP: the full IPv4 address, or the IPv6 /64
    /// (one subscriber's usual allocation). Never the raw IP.
    pub fn rate_limit_ip_key(&self, ip: IpAddr) -> [u8; 16] {
        let prefix = match canonical(ip) {
            IpAddr::V4(v4) => {
                let mut v = vec![4];
                v.extend_from_slice(&v4.octets());
                v
            }
            IpAddr::V6(v6) => {
                let mut v = vec![6];
                v.extend_from_slice(&v6.octets()[..8]);
                v
            }
        };
        truncate16(hmac(&self.ip, &[b"rate", &prefix]))
    }

    /// The rate-limit key for an email address (already normalised).
    pub fn rate_limit_email_key(&self, email: &str) -> [u8; 16] {
        truncate16(hmac(&self.ip, &[b"email", email.as_bytes()]))
    }

    /// The unsubscribe token for a signup: base64url(id ‖ HMAC(id)[..16]).
    pub fn leave_token(&self, signup_id: Uuid) -> String {
        let tag = hmac(&self.leave, &[signup_id.as_bytes()]);
        let mut raw = Vec::with_capacity(16 + LEAVE_TAG_LEN);
        raw.extend_from_slice(signup_id.as_bytes());
        raw.extend_from_slice(&tag[..LEAVE_TAG_LEN]);
        URL_SAFE_NO_PAD.encode(raw)
    }

    /// Checks a leave token and returns the signup id it names.
    pub fn verify_leave_token(&self, token: &str) -> Option<Uuid> {
        if token.len() > 64 {
            return None;
        }
        let raw = URL_SAFE_NO_PAD.decode(token.trim()).ok()?;
        if raw.len() != 16 + LEAVE_TAG_LEN {
            return None;
        }
        let (id, tag) = raw.split_at(16);
        let mut mac = HmacSha256::new_from_slice(&self.leave).expect("any key length");
        mac.update(id);
        mac.verify_truncated_left(tag).ok()?;
        Uuid::from_slice(id).ok()
    }
}

fn truncate16(full: [u8; 32]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&full[..16]);
    out
}

/// IPv4-mapped IPv6 addresses count as IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        v4 => v4,
    }
}

/// A fresh confirmation token: 32 random bytes, base64url (43 characters).
pub struct ConfirmationToken {
    pub token: String,
    pub hash: [u8; 32],
}

impl ConfirmationToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the OS random source is available");
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let hash = hash_token(&token);
        Self { token, hash }
    }
}

/// SHA-256 of a token as it appears in the link.
pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Whether `token` looks like one we issued (43 base64url characters).
pub fn is_well_formed_token(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(seed: u8) -> Keys {
        Keys::derive(&SecretKey::new(vec![seed; 64]))
    }

    #[test]
    fn leave_token_round_trips_and_rejects_tampering() {
        let k = keys(1);
        let id = Uuid::new_v4();
        let t = k.leave_token(id);
        assert_eq!(k.verify_leave_token(&t), Some(id));
        assert_eq!(keys(2).verify_leave_token(&t), None, "other secret");
        let mut bad = t.into_bytes();
        bad[40] = if bad[40] == b'A' { b'B' } else { b'A' };
        assert_eq!(k.verify_leave_token(std::str::from_utf8(&bad).unwrap()), None);
        assert_eq!(k.verify_leave_token("nope"), None);
    }

    #[test]
    fn confirmation_tokens_are_random_and_well_formed() {
        let a = ConfirmationToken::generate();
        let b = ConfirmationToken::generate();
        assert_ne!(a.token, b.token);
        assert!(is_well_formed_token(&a.token));
        assert_eq!(hash_token(&a.token), a.hash);
        assert!(!is_well_formed_token("short"));
    }

    #[test]
    fn ip_hashes_are_coarse_and_keyed() {
        let k = keys(1);
        let a: IpAddr = "203.0.113.7".parse().unwrap();
        let b: IpAddr = "203.0.113.200".parse().unwrap();
        let mapped: IpAddr = "::ffff:203.0.113.7".parse().unwrap();
        assert_eq!(k.coarse_ip_hash(a), k.coarse_ip_hash(b));
        assert_ne!(k.rate_limit_ip_key(a), k.rate_limit_ip_key(b));
        assert_eq!(k.rate_limit_ip_key(a), k.rate_limit_ip_key(mapped));
        assert_ne!(k.coarse_ip_hash(a), keys(2).coarse_ip_hash(a));
    }
}
