//! Notification domain invariants and state-machine property tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::{Duration, Utc};
use openpanel_domain::notifications::{
    Channel, ChannelKind, DeliveryAttempt, DeliveryStatus, EventFilter, EventKind,
    NotificationEvent, Severity, Subscription, TlsMode, retry_delay, sign_webhook,
};
use proptest::prelude::*;
use uuid::Uuid;

#[test]
fn channels_validate_and_metadata_redacts_encrypted_credentials() {
    let smtp = Channel::smtp(
        Uuid::new_v4(),
        "primary mail".into(),
        "smtp.example.test".into(),
        587,
        "mailer".into(),
        "ciphertext-password".into(),
        "sender@example.test".into(),
        TlsMode::StartTls,
        vec!["ops@example.test".into()],
        Utc::now(),
    )
    .unwrap();
    assert_eq!(smtp.kind(), ChannelKind::Smtp);
    let json = serde_json::to_string(&smtp.metadata()).unwrap();
    assert!(!json.contains("ciphertext-password"));

    let webhook = Channel::webhook(
        Uuid::new_v4(),
        "pager".into(),
        "https://192.0.2.10/hook".into(),
        "ciphertext-signing-key".into(),
        vec!["192.0.2.0/24".into()],
        Utc::now(),
    )
    .unwrap();
    assert_eq!(webhook.kind(), ChannelKind::Webhook);
    assert!(
        !serde_json::to_string(&webhook.metadata())
            .unwrap()
            .contains("ciphertext-signing-key")
    );
    assert!(
        Channel::webhook(
            Uuid::new_v4(),
            "bad".into(),
            "http://example.test/hook".into(),
            "ciphertext".into(),
            vec![],
            Utc::now(),
        )
        .is_err()
    );
}

#[test]
fn subscription_filters_match_only_requested_events() {
    let filter = EventFilter::from_json(
        EventKind::Alert,
        serde_json::json!({
            "metrics": ["cpu_percent"],
            "severity_at_least": "warning"
        }),
    )
    .unwrap();
    let subscription = Subscription::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "ops@example.test".into(),
        EventKind::Alert,
        filter,
        Utc::now(),
    )
    .unwrap();
    let matching = NotificationEvent::new(
        Uuid::new_v4(),
        EventKind::Alert,
        "cpu high".into(),
        Severity::Critical,
        serde_json::json!({"metric":"cpu_percent","value":95}),
        Utc::now(),
    )
    .unwrap();
    let wrong_metric = NotificationEvent::new(
        Uuid::new_v4(),
        EventKind::Alert,
        "memory high".into(),
        Severity::Critical,
        serde_json::json!({"metric":"memory_percent","value":95}),
        Utc::now(),
    )
    .unwrap();
    assert!(subscription.matches(&matching));
    assert!(!subscription.matches(&wrong_metric));
    let mut disabled = subscription.clone();
    disabled.disable();
    assert!(!disabled.matches(&matching));
}

#[test]
fn webhook_signature_and_payload_are_stable_bounded_and_secret_free() {
    let event = NotificationEvent::new(
        Uuid::new_v4(),
        EventKind::Audit,
        "login denied".into(),
        Severity::Warning,
        serde_json::json!({"action":"login_failure","password":"must disappear"}),
        Utc::now(),
    )
    .unwrap();
    let body = event.webhook_body(Uuid::nil(), 1024).unwrap();
    assert!(body.len() <= 1024);
    assert!(!String::from_utf8_lossy(&body).contains("must disappear"));
    assert_eq!(
        sign_webhook(b"signing-secret", &body),
        sign_webhook(b"signing-secret", &body)
    );
    assert_ne!(
        sign_webhook(b"signing-secret", &body),
        sign_webhook(b"different-secret", &body)
    );
}

#[test]
fn delivery_retry_schedule_and_lease_recovery_preserve_identity() {
    let now = Utc::now();
    let mut delivery = DeliveryAttempt::pending(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "digest".into(),
        now,
    );
    let id = delivery.id();
    assert_eq!(
        (1..=5).map(retry_delay).collect::<Vec<_>>(),
        vec![
            Duration::minutes(1),
            Duration::minutes(5),
            Duration::minutes(30),
            Duration::hours(2),
            Duration::hours(12),
        ]
    );
    delivery.lease(now, Duration::seconds(30)).unwrap();
    assert_eq!(delivery.status(), DeliveryStatus::Leased);
    assert!(!delivery.recover_expired_lease(now + Duration::seconds(29)));
    assert!(delivery.recover_expired_lease(now + Duration::seconds(31)));
    assert_eq!(delivery.id(), id);
    delivery
        .lease(now + Duration::seconds(31), Duration::seconds(30))
        .unwrap();
    delivery
        .record_transient_failure(now + Duration::seconds(31), "timeout")
        .unwrap();
    assert_eq!(delivery.attempt_n(), 1);
    assert_eq!(delivery.status(), DeliveryStatus::Retry);
    assert_eq!(
        delivery.next_retry_at(),
        Some(now + Duration::seconds(31) + Duration::minutes(1))
    );
}

#[test]
fn terminal_delivery_cannot_transition_or_be_released_twice() {
    let now = Utc::now();
    let mut delivery = DeliveryAttempt::pending(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "digest".into(),
        now,
    );
    delivery.lease(now, Duration::seconds(30)).unwrap();
    delivery.record_success(now).unwrap();
    assert_eq!(delivery.status(), DeliveryStatus::TerminalSuccess);
    assert!(delivery.record_success(now).is_err());
    assert!(delivery.record_transient_failure(now, "late").is_err());
}

proptest! {
    #[test]
    fn prop_backoff_is_monotonic(attempt_a in 1_u32..6, attempt_b in 1_u32..6) {
        if attempt_a < attempt_b {
            prop_assert!(retry_delay(attempt_a) < retry_delay(attempt_b));
        }
    }

    #[test]
    fn prop_signature_changes_when_body_changes(body in prop::collection::vec(any::<u8>(), 0..1024), suffix in any::<u8>()) {
        let mut changed = body.clone();
        changed.push(suffix);
        prop_assert_ne!(sign_webhook(b"key", &body), sign_webhook(b"key", &changed));
    }

    #[test]
    fn prop_empty_alert_filter_never_matches(metric in "[a-z_]{1,32}") {
        let filter = EventFilter::from_json(EventKind::Alert, serde_json::json!({})).unwrap();
        let event = NotificationEvent::new(
            Uuid::new_v4(),
            EventKind::Alert,
            "subject".into(),
            Severity::Warning,
            serde_json::json!({"metric":metric}),
            Utc::now(),
        ).unwrap();
        prop_assert!(!filter.matches(&event));
    }
}
