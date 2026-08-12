//! Remember-device cookie: stateless HMAC-signed payload that lets a
//! verified browser skip the second-factor challenge on future logins
//! for the same factor on the same device fingerprint.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use openpanel_domain::identity::{
    REMEMBER_DEVICE_VERSION, RememberedDevicePayload, hash_user_agent, ip_prefix,
};
use sha2::Sha256;
use uuid::Uuid;

use super::two_factor::TwoFactorError;

/// Number of seconds in a day, used for the cookie `Max-Age`.
const SECONDS_PER_DAY: i64 = 86_400;
/// Default cookie lifetime in days (30 days matches the spec default).
const DEFAULT_LIFETIME_DAYS: i64 = 30;

/// HMAC key derived from the master key via HKDF-SHA256. Stored as a
/// fixed-length byte string so verification is constant-time.
fn derive_signing_key(master_key: &[u8; 32]) -> [u8; 32] {
    // HKDF-Extract: PRK = HMAC-SHA256(salt, master_key). The salt is
    // empty (zeros) to keep the derivation deterministic for a given
    // master key; we don't need per-cookie uniqueness in the key space.
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = match <HmacSha256 as Mac>::new_from_slice(master_key) {
        Ok(value) => value,
        // HMAC accepts any key length; this branch is unreachable.
        Err(_) => unreachable!("HMAC accepts any key length"),
    };
    mac.update(&[0u8; 32]);
    let mut prk = mac.finalize().into_bytes();
    // HKDF-Expand: OKM = HMAC(PRK, info || 0x01) (single block, 32 bytes).
    let mut mac2 = match <HmacSha256 as Mac>::new_from_slice(&prk) {
        Ok(value) => value,
        Err(_) => unreachable!("HMAC accepts any key length"),
    };
    mac2.update(b"openpanel_2fa_remember");
    mac2.update(&[0x01]);
    let okm = mac2.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&okm);
    // Zeroize the intermediate PRK (best-effort; the macro derive is a
    // newtype around Hmac<Sha256>).
    for b in prk.iter_mut() {
        *b = 0;
    }
    out
}

fn sign(cookie_key: &[u8; 32], payload_bytes: &[u8]) -> [u8; 32] {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = match <HmacSha256 as Mac>::new_from_slice(cookie_key) {
        Ok(value) => value,
        Err(_) => unreachable!("HMAC accepts any key length"),
    };
    mac.update(payload_bytes);
    let tag = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&tag);
    out
}

/// Build a remember-device cookie for `factor_id` bound to `user_agent`
/// and `ip`. Returns the cookie value (the panel sets the cookie itself).
pub fn issue_remember_device(
    master_key: &[u8; 32],
    user_id: Uuid,
    factor_id: Uuid,
    user_agent: &str,
    ip: &str,
    now: DateTime<Utc>,
) -> String {
    issue_remember_device_with_lifetime(
        master_key,
        user_id,
        factor_id,
        user_agent,
        ip,
        now,
        chrono::Duration::days(DEFAULT_LIFETIME_DAYS),
    )
}

/// Build a remember-device cookie using a configured lifetime.
pub fn issue_remember_device_with_lifetime(
    master_key: &[u8; 32],
    user_id: Uuid,
    factor_id: Uuid,
    user_agent: &str,
    ip: &str,
    now: DateTime<Utc>,
    lifetime: chrono::Duration,
) -> String {
    let cookie_key = derive_signing_key(master_key);
    let payload = RememberedDevicePayload {
        v: REMEMBER_DEVICE_VERSION,
        factor_id,
        user_id,
        ua_hash: hash_user_agent(user_agent),
        ip_prefix: ip_prefix(ip),
        exp: now + lifetime,
        nonce: Uuid::new_v4(),
    };
    let payload_bytes = match serde_json::to_vec(&payload) {
        Ok(value) => value,
        // Our payload is a fixed-shape struct; serialisation only fails
        // on allocation failure, which we treat as fatal.
        Err(_) => unreachable!("payload is serializable"),
    };
    let tag = sign(&cookie_key, &payload_bytes);
    let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_bytes);
    let tag_b64 = URL_SAFE_NO_PAD.encode(tag);
    format!("{payload_b64}.{tag_b64}")
}

/// Validate a remember-device cookie. Returns the payload on success,
/// or a typed error when the cookie is missing, malformed, tampered,
/// expired, or bound to a different UA / IP prefix.
pub fn validate_remember_device(
    master_key: &[u8; 32],
    cookie_value: &str,
    user_agent: &str,
    ip: &str,
    now: DateTime<Utc>,
) -> Result<RememberedDevicePayload, TwoFactorError> {
    let (payload_b64, tag_b64) = cookie_value
        .split_once('.')
        .ok_or(TwoFactorError::RememberDeviceInvalid)?;
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| TwoFactorError::RememberDeviceInvalid)?;
    let expected_tag = URL_SAFE_NO_PAD
        .decode(tag_b64)
        .map_err(|_| TwoFactorError::RememberDeviceInvalid)?;
    let cookie_key = derive_signing_key(master_key);
    let actual_tag = sign(&cookie_key, &payload_bytes);
    if !constant_time_eq_32(&actual_tag, &expected_tag) {
        return Err(TwoFactorError::RememberDeviceInvalid);
    }
    let payload: RememberedDevicePayload = serde_json::from_slice(&payload_bytes)
        .map_err(|_| TwoFactorError::RememberDeviceInvalid)?;
    if payload.v != REMEMBER_DEVICE_VERSION {
        return Err(TwoFactorError::RememberDeviceInvalid);
    }
    if !payload.is_live(now) {
        return Err(TwoFactorError::RememberDeviceExpired);
    }
    if !payload.matches_request(user_agent, ip_prefix(ip).as_str()) {
        return Err(TwoFactorError::RememberDeviceMismatch);
    }
    Ok(payload)
}

/// Cookie Max-Age in seconds, matching the issue-time lifetime.
pub fn remember_device_max_age_seconds() -> u64 {
    (DEFAULT_LIFETIME_DAYS * SECONDS_PER_DAY) as u64
}

fn constant_time_eq_32(a: &[u8; 32], b: &[u8]) -> bool {
    if b.len() != 32 {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_then_validate_round_trips() {
        let master = [0x42u8; 32];
        let user = Uuid::new_v4();
        let factor = Uuid::new_v4();
        let ua = "Mozilla/5.0 (Test)";
        let ip = "203.0.113.42";
        let cookie = issue_remember_device(&master, user, factor, ua, ip, Utc::now());
        let payload = validate_remember_device(&master, &cookie, ua, ip, Utc::now()).unwrap();
        assert_eq!(payload.user_id, user);
        assert_eq!(payload.factor_id, factor);
    }

    #[test]
    fn tampering_breaks_signature() {
        let master = [0x42u8; 32];
        let cookie = issue_remember_device(
            &master,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "ua",
            "10.0.0.1",
            Utc::now(),
        );
        // Flip one character in the payload half.
        let (payload, tag) = cookie.split_once('.').unwrap();
        let mut bytes = payload.as_bytes().to_vec();
        bytes[0] = if bytes[0] == b'A' { b'B' } else { b'A' };
        let tampered = format!("{}.{}", String::from_utf8(bytes).unwrap(), tag);
        assert!(matches!(
            validate_remember_device(&master, &tampered, "ua", "10.0.0.1", Utc::now()),
            Err(TwoFactorError::RememberDeviceInvalid)
        ));
    }

    #[test]
    fn mismatched_user_agent_rejects() {
        let master = [0x42u8; 32];
        let cookie = issue_remember_device(
            &master,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "ua-A",
            "10.0.0.1",
            Utc::now(),
        );
        assert!(matches!(
            validate_remember_device(&master, &cookie, "ua-B", "10.0.0.1", Utc::now()),
            Err(TwoFactorError::RememberDeviceMismatch)
        ));
    }

    #[test]
    fn mismatched_ip_prefix_rejects() {
        let master = [0x42u8; 32];
        let cookie = issue_remember_device(
            &master,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "ua",
            "10.0.0.1",
            Utc::now(),
        );
        assert!(matches!(
            validate_remember_device(&master, &cookie, "ua", "192.168.0.1", Utc::now()),
            Err(TwoFactorError::RememberDeviceMismatch)
        ));
    }

    #[test]
    fn expired_cookie_rejects() {
        let master = [0x42u8; 32];
        let issued_at = Utc::now() - chrono::Duration::days(60);
        let cookie = issue_remember_device(
            &master,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "ua",
            "10.0.0.1",
            issued_at,
        );
        assert!(matches!(
            validate_remember_device(&master, &cookie, "ua", "10.0.0.1", Utc::now()),
            Err(TwoFactorError::RememberDeviceExpired)
        ));
    }

    #[test]
    fn different_master_keys_do_not_validate() {
        let cookie = issue_remember_device(
            &[0x01u8; 32],
            Uuid::new_v4(),
            Uuid::new_v4(),
            "ua",
            "10.0.0.1",
            Utc::now(),
        );
        assert!(matches!(
            validate_remember_device(&[0x02u8; 32], &cookie, "ua", "10.0.0.1", Utc::now()),
            Err(TwoFactorError::RememberDeviceInvalid)
        ));
    }
}
