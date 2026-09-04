//! Tests for the audit log: redaction, cursor round-trips, and queries.

use super::*;

fn sample(action: AuditAction, outcome: AuditOutcome, meta: Value) -> AuditEvent {
    AuditEvent::new("admin", action, outcome)
        .target("site.example")
        .metadata(meta)
}

#[test]
fn redaction_drops_secret_keys() {
    let md = serde_json::json!({
        "password": "hunter2",
        "token": "abc.def.ghi",
        "target_id": "1234",
        "note": "hello",
    });
    let view = AuditView::from_event(&sample(AuditAction::UserCreated, AuditOutcome::Success, md));
    assert!(!view.metadata.as_object().unwrap().contains_key("password"));
    assert!(!view.metadata.as_object().unwrap().contains_key("token"));
    assert_eq!(view.metadata["target_id"], "1234");
    assert_eq!(view.metadata["note"], "hello");
}

#[test]
fn redaction_scrubs_secret_values() {
    let md = serde_json::json!({
        "note": "normal text",
        "body": "-----BEGIN PRIVATE KEY-----\nMIIE...\n-----END PRIVATE KEY-----",
        "jwt": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
    });
    let view = AuditView::from_event(&sample(
        AuditAction::SettingsChanged,
        AuditOutcome::Success,
        md,
    ));
    // Secret-shaped keys are dropped entirely.
    assert!(!view.metadata.as_object().unwrap().contains_key("body"));
    // Non-secret keys holding secret-shaped values are redacted in place.
    assert_eq!(view.metadata["note"], "normal text");
    assert_eq!(view.metadata["jwt"], "***redacted***");
}

#[test]
fn redaction_recurses_into_nested_objects() {
    let md = serde_json::json!({
        "outer": { "api_key": "secret", "ok": "kept" },
    });
    let view = AuditView::from_event(&sample(AuditAction::SslIssued, AuditOutcome::Success, md));
    let outer = view.metadata["outer"].as_object().unwrap();
    assert!(!outer.contains_key("api_key"));
    assert_eq!(outer["ok"], "kept");
}

#[test]
fn cursor_round_trips() {
    let c = AuditCursor {
        ts: Utc::now(),
        id: 42,
    };
    let encoded = c.encode();
    let decoded = AuditCursor::decode(&encoded).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn cursor_is_stable_across_round_trip_with_millis() {
    let c = AuditCursor {
        ts: DateTime::parse_from_rfc3339("2026-08-29T12:34:56.789Z")
            .unwrap()
            .with_timezone(&Utc),
        id: 7,
    };
    assert_eq!(AuditCursor::decode(&c.encode()), Some(c));
}

#[test]
fn query_default_limit_is_positive() {
    let q = AuditQuery::new();
    assert!(q.effective_limit() > 1);
}

#[test]
fn query_with_cursor_ignores_garbage() {
    let q = AuditQuery::new().with_cursor(Some("not-a-cursor"));
    assert!(q.cursor.is_none());
}
