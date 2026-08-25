//! Unit and property tests for SSO domain types.

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{ExternalIdentity, STATE_TTL_SECS, SsoConnection, SsoError, SsoLoginState};

fn connection() -> SsoConnection {
    SsoConnection {
        issuer_url: "https://idp.example.com".into(),
        client_id: "openpanel".into(),
        client_secret_cipher: "a1b2c3d4e5f60718293a4b5c6d7e8f90:0f9e8d7c6b5a4938271605049382716a"
            .into(),
        default_role: crate::Role::User,
        auto_provision: false,
        trust_idp_mfa: false,
    }
}

#[test]
fn test_connection_accepts_valid_config() {
    connection().validate().unwrap();
}

#[test]
fn test_connection_rejects_bad_issuer_and_plain_secret() {
    let mut bad = connection();
    bad.issuer_url = "ftp://idp.example.com".into();
    assert!(matches!(bad.validate(), Err(SsoError::InvalidConfig(_))));

    let mut plaintext = connection();
    plaintext.client_secret_cipher = "super-secret".into();
    assert!(matches!(
        plaintext.validate(),
        Err(SsoError::InvalidConfig(_))
    ));
}

#[test]
fn test_mfa_interplay_follows_flag() {
    let mut conn = connection();
    assert!(!conn.mfa_satisfied());
    conn.trust_idp_mfa = true;
    assert!(conn.mfa_satisfied());
}

#[test]
fn test_validate_callback_happy_path() {
    let now = Utc::now();
    let state = SsoLoginState::new(now);
    state
        .validate_callback(&state.state, &state.nonce, now)
        .unwrap();
}

#[test]
fn test_validate_callback_rejects_mismatch_expiry_nonce() {
    let now = Utc::now();
    let state = SsoLoginState::new(now);

    // Wrong state.
    assert_eq!(
        state
            .validate_callback("wrong", &state.nonce, now)
            .unwrap_err(),
        SsoError::StateMismatch
    );

    // Expired.
    let later = now + Duration::seconds(STATE_TTL_SECS + 1);
    assert_eq!(
        state
            .validate_callback(&state.state, &state.nonce, later)
            .unwrap_err(),
        SsoError::StateExpired
    );

    // Nonce mismatch (state itself valid).
    assert_eq!(
        state
            .validate_callback(&state.state, "wrong", now)
            .unwrap_err(),
        SsoError::NonceMismatch
    );
}

#[test]
fn test_states_are_unique_across_mints() {
    let now = Utc::now();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1000 {
        let state = SsoLoginState::new(now);
        assert!(seen.insert(state.state.clone()), "state collision");
    }
}

#[test]
fn test_external_identity_round_trips() {
    let identity = ExternalIdentity {
        issuer: "https://idp.example.com".into(),
        subject: "auth0|12345".into(),
        user_id: Uuid::new_v4(),
    };
    let json = serde_json::to_string(&identity).unwrap();
    let parsed: ExternalIdentity = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, identity);
}
