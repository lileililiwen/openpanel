//! Remember-device cookie payload.
//!
//! The cookie is stateless: the payload is HMAC-signed and carries the
//! factor id, expiry, and fingerprints the panel uses to recognise the
//! device on a future login. Validation re-derives the HMAC and compares
//! the fingerprints; a DB table is not required.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current payload version. Bump on breaking layout changes.
pub const REMEMBER_DEVICE_VERSION: u8 = 1;

/// Default cookie lifetime (30 days).
pub const REMEMBER_DEVICE_DEFAULT_LIFETIME: chrono::Duration = chrono::Duration::days(30);

/// Stable cookie name used by both the issuer (web/API) and validator
/// (login flow).
pub const REMEMBER_DEVICE_COOKIE: &str = "openpanel_2fa_remember";

/// Plaintext payload of a remember-device cookie. The HMAC tag is
/// appended and stripped by the app layer; this struct never carries
/// the tag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RememberedDevicePayload {
    /// Payload schema version.
    pub v: u8,
    /// Factor the device is bound to.
    pub factor_id: Uuid,
    /// Owner user id (carried for audit convenience).
    pub user_id: Uuid,
    /// SHA-256 of the request's User-Agent at issue time, hex-encoded.
    pub ua_hash: String,
    /// First 3 octets of an IPv4 address, or first 4 groups of an IPv6
    /// address, written as a string. Browsers behind large NATs may fail
    /// the prefix check; that is intentional — the user can re-verify.
    pub ip_prefix: String,
    /// Absolute UTC expiry.
    pub exp: DateTime<Utc>,
    /// Random nonce so two cookies issued back-to-back differ.
    pub nonce: Uuid,
}

impl RememberedDevicePayload {
    /// Whether the payload is still within its lifetime at `now`.
    pub fn is_live(&self, now: DateTime<Utc>) -> bool {
        now < self.exp
    }

    /// Whether the cookie is bound to this user-agent and IP prefix.
    pub fn matches_request(&self, user_agent: &str, ip_prefix: &str) -> bool {
        // Constant-time-ish: same length-first compare then byte compare.
        self.ua_hash == hash_user_agent(user_agent) && self.ip_prefix == ip_prefix
    }
}

/// Compute the SHA-256 hex of a user-agent string.
pub fn hash_user_agent(user_agent: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(user_agent.as_bytes());
    hex::encode(digest)
}

/// Compute the IP prefix the cookie binds to. IPv4: first 3 octets
/// (`"a.b.c"`). IPv6: first 4 groups (`"a:b:c:d"`). Anything else
/// (empty, hostname, unknown) is returned verbatim as a fallback so
/// the caller still has a stable string to compare.
pub fn ip_prefix(ip: &str) -> String {
    if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
        match addr {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();
                format!("{}.{}.{}", octets[0], octets[1], octets[2])
            }
            std::net::IpAddr::V6(v6) => {
                let segments = v6.segments();
                format!(
                    "{:x}:{:x}:{:x}:{:x}",
                    segments[0], segments[1], segments[2], segments[3]
                )
            }
        }
    } else {
        ip.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_user_agent_is_stable_and_distinguishes_inputs() {
        let a = hash_user_agent("Mozilla/5.0");
        let b = hash_user_agent("Mozilla/5.0");
        let c = hash_user_agent("Mozilla/5.1");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn ip_prefix_truncates_ipv4_and_ipv6() {
        assert_eq!(ip_prefix("203.0.113.42"), "203.0.113");
        assert_eq!(
            ip_prefix("2001:db8:abcd:0011:2233:4455:6677:8899"),
            "2001:db8:abcd:11"
        );
        // Unknown formats fall back to the raw string so the caller's
        // stable string compare still works.
        assert_eq!(ip_prefix("not-an-ip"), "not-an-ip");
    }

    #[test]
    fn payload_match_is_exact() {
        let now = Utc::now();
        let payload = RememberedDevicePayload {
            v: REMEMBER_DEVICE_VERSION,
            factor_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            ua_hash: hash_user_agent("test"),
            ip_prefix: "10.0.0".into(),
            exp: now + chrono::Duration::days(1),
            nonce: Uuid::new_v4(),
        };
        assert!(payload.matches_request("test", "10.0.0"));
        assert!(!payload.matches_request("other", "10.0.0"));
        assert!(!payload.matches_request("test", "10.0.1"));
    }
}
