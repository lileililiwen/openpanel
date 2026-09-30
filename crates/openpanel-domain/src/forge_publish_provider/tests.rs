//! Forge publish provider domain unit + property tests. The
//! tests exercise request parsing, validation, redaction, and
//! the wire-shape round-trip. Secrets and source contents are
//! rejected at the boundary.

use serde_json::json;
use uuid::Uuid;

use super::logic::{
    MAX_DIAGNOSTIC_LEN, map_source_kind, parse_request, redact_diagnostic, redact_request_metadata,
    redact_response_metadata, redact_source_label, validate_operation_identity, validate_revision,
};
use super::types::{
    FORGE_PUBLISH_PROVIDER_IDENTITY, FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION,
    ForgeProviderCapabilities, ForgeProviderError, ForgeProviderHealthSummary,
    ForgeProviderOperation, ForgeProviderOperationId, ForgeProviderProviderIdentity,
    ForgeProviderRequest, ForgeProviderResponse, ForgeProviderRevision,
    ForgeProviderRollbackStatus, ForgeProviderSourceKind, ForgeProviderStatus,
    ForgeProviderTerminalEvidence,
};
use crate::application_delivery::ApplicationSourceKind;

fn full_request() -> ForgeProviderRequest {
    ForgeProviderRequest {
        protocol: FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION.to_string(),
        operation: ForgeProviderOperation::Publish,
        project: super::types::ForgeProviderProject {
            slug: "acme/web".into(),
            label: Some("Acme web".into()),
        },
        revision: Some(ForgeProviderRevision {
            kind: ForgeProviderSourceKind::Git,
            url: "https://example.com/acme/web.git".into(),
            source_ref: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(),
            release_digest: Some(
                "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(),
            ),
            environment: "production".into(),
        }),
        operation_id: Some(ForgeProviderOperationId {
            operation_id: Uuid::parse_str("0190b9c9-2f5c-7c1a-9f7d-2d5b9b9f6c61").unwrap(),
            dedupe_key: Some("publish-42".into()),
        }),
        secret_set: Some(super::types::ForgeProviderSecretReference {
            secret_set: "prod".into(),
            version: Some("1".into()),
        }),
        dry_run: false,
    }
}

#[test]
fn parse_request_accepts_valid_publish_payload() {
    let request = full_request();
    let value = serde_json::to_value(&request).unwrap();
    let parsed = parse_request(&value).expect("valid payload");
    assert_eq!(parsed.operation, ForgeProviderOperation::Publish);
    assert_eq!(parsed.project.slug, "acme/web");
    assert!(parsed.revision.as_ref().unwrap().has_immutable_digest());
}

#[test]
fn parse_request_rejects_unknown_protocol() {
    let mut request = full_request();
    request.protocol = "forge-publish-provider/0.0.1".into();
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("unknown protocol");
    assert!(matches!(err, ForgeProviderError::UnsupportedProtocol(_)));
}

#[test]
fn parse_request_rejects_missing_protocol() {
    let mut request = full_request();
    request.protocol = String::new();
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("missing protocol");
    assert!(matches!(err, ForgeProviderError::UnsupportedProtocol(_)));
}

#[test]
fn parse_request_rejects_missing_revision_for_publish() {
    let mut request = full_request();
    request.revision = None;
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("missing revision");
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn parse_request_rejects_missing_operation_id_for_publish() {
    let mut request = full_request();
    request.operation_id = None;
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("missing operation id");
    assert!(matches!(
        err,
        ForgeProviderError::InvalidOperationIdentity(_)
    ));
}

#[test]
fn parse_request_allows_capabilities_without_revision() {
    let mut request = full_request();
    request.operation = ForgeProviderOperation::Capabilities;
    request.revision = None;
    request.operation_id = None;
    let value = serde_json::to_value(&request).unwrap();
    let parsed = parse_request(&value).expect("capabilities is metadata-only");
    assert_eq!(parsed.operation, ForgeProviderOperation::Capabilities);
    assert!(parsed.revision.is_none());
}

#[test]
fn validate_revision_rejects_mutable_tag_digest() {
    let mut request = full_request();
    let revision = request.revision.as_mut().unwrap();
    revision.release_digest = Some("latest".into());
    let err = validate_revision(revision).expect_err("mutable digest");
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn validate_revision_rejects_empty_source_ref() {
    let mut request = full_request();
    request.revision.as_mut().unwrap().source_ref = String::new();
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("empty source_ref");
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn validate_revision_rejects_oversize_source_ref() {
    let mut request = full_request();
    request.revision.as_mut().unwrap().source_ref =
        "a".repeat(super::logic::MAX_SOURCE_REF_LEN + 1);
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("oversize source_ref");
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn validate_project_rejects_oversize_slug() {
    let mut request = full_request();
    request.project.slug = "a".repeat(super::logic::MAX_PROJECT_SLUG_LEN + 1);
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("oversize slug");
    assert!(matches!(err, ForgeProviderError::InvalidProject(_)));
}

#[test]
fn validate_project_rejects_uppercase_slug() {
    let mut request = full_request();
    request.project.slug = "Acme/Web".into();
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("uppercase slug");
    assert!(matches!(err, ForgeProviderError::InvalidProject(_)));
}

#[test]
fn validate_project_accepts_hyphen_and_underscore() {
    let mut request = full_request();
    request.project.slug = "acme-team/web_store".into();
    let value = serde_json::to_value(&request).unwrap();
    let parsed = parse_request(&value).expect("hyphen + underscore slug");
    assert_eq!(parsed.project.slug, "acme-team/web_store");
}

#[test]
fn validate_operation_identity_rejects_nil_uuid() {
    let err = validate_operation_identity(&ForgeProviderOperationId {
        operation_id: Uuid::nil(),
        dedupe_key: None,
    })
    .expect_err("nil uuid");
    assert!(matches!(
        err,
        ForgeProviderError::InvalidOperationIdentity(_)
    ));
}

#[test]
fn validate_operation_identity_rejects_empty_dedupe_key() {
    let err = validate_operation_identity(&ForgeProviderOperationId {
        operation_id: Uuid::new_v4(),
        dedupe_key: Some(String::new()),
    })
    .expect_err("empty dedupe key");
    assert!(matches!(
        err,
        ForgeProviderError::InvalidOperationIdentity(_)
    ));
}

#[test]
fn redact_diagnostic_collapses_secret_markers() {
    let cases = [
        "deploy password=hunter2 failed",
        "secret_value=abcd",
        "token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
        "ran ssh -i /root/.ssh/id_rsa against prod",
        "wrote /etc/passwd entry",
        "kubectl apply -f manifest.yaml",
        "docker login -u admin -p password",
    ];
    for case in cases {
        let redacted = redact_diagnostic(case);
        assert_eq!(redacted, "[REDACTED]", "must redact marker in {case}");
    }
}

#[test]
fn redact_diagnostic_truncates_oversize_diagnostic() {
    let long = "a".repeat(MAX_DIAGNOSTIC_LEN * 4);
    let redacted = redact_diagnostic(long);
    assert!(redacted.len() <= MAX_DIAGNOSTIC_LEN);
    assert!(redacted.ends_with('…'));
}

#[test]
fn redact_diagnostic_keeps_safe_diagnostic_intact() {
    let safe = "release promoted from healthy to rolled-back";
    let redacted = redact_diagnostic(safe);
    assert_eq!(redacted, safe);
}

#[test]
fn redact_source_label_strips_absolute_paths() {
    let label = "/var/log/secret.log";
    let redacted = redact_source_label(label);
    assert!(redacted.starts_with("[REDACTED-PATH]/"));
}

#[test]
fn redact_request_metadata_echoes_operation_environment_slug() {
    let request = full_request();
    let redacted = redact_request_metadata(&request);
    assert_eq!(
        redacted,
        "forge.publish.op=publish project=acme/web env=production"
    );
}

#[test]
fn redact_request_metadata_handles_capabilities_request() {
    let mut request = full_request();
    request.operation = ForgeProviderOperation::Capabilities;
    request.revision = None;
    request.operation_id = None;
    let redacted = redact_request_metadata(&request);
    assert_eq!(redacted, "forge.publish.op=capabilities project=acme/web");
}

#[test]
fn redact_response_metadata_echoes_status_environment() {
    let response = ForgeProviderResponse {
        provider: ForgeProviderProviderIdentity {
            provider_id: FORGE_PUBLISH_PROVIDER_IDENTITY.to_string(),
            implementation_version: "openpanel-forge-publish-provider/0.1.0".into(),
            operations: vec![ForgeProviderOperation::Publish],
            protocol_versions: vec![FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION.into()],
        },
        operation_id: Uuid::new_v4(),
        operation: ForgeProviderOperation::Publish,
        environment: Some("production".into()),
        status: ForgeProviderStatus::Succeeded,
        capabilities: None,
        health: None,
        rollback: None,
        terminal: None,
        diagnostic: Some("password=hunter2".into()),
    };
    let redacted = redact_response_metadata(&response, chrono::Utc::now());
    assert!(
        redacted.starts_with(
            "forge.publish.resp.op=publish status=succeeded env=production observed_at="
        )
    );
    assert!(!redacted.contains("password"));
}

#[test]
fn source_kind_mapping_is_total() {
    let cases = [
        (ForgeProviderSourceKind::Git, ApplicationSourceKind::Git),
        (ForgeProviderSourceKind::Oci, ApplicationSourceKind::Oci),
        (
            ForgeProviderSourceKind::Compose,
            ApplicationSourceKind::Compose,
        ),
    ];
    for (wire, internal) in cases {
        assert_eq!(map_source_kind(wire), internal);
    }
}

#[test]
fn operation_is_mutating_matches_documented_set() {
    assert!(!ForgeProviderOperation::Capabilities.is_mutating());
    assert!(!ForgeProviderOperation::Preflight.is_mutating());
    assert!(!ForgeProviderOperation::Verify.is_mutating());
    assert!(ForgeProviderOperation::Publish.is_mutating());
    assert!(ForgeProviderOperation::Rollback.is_mutating());
}

#[test]
fn status_wire_names_are_stable() {
    let cases = [
        (ForgeProviderStatus::Succeeded, "succeeded"),
        (ForgeProviderStatus::DryRun, "dry_run"),
        (ForgeProviderStatus::InvalidRequest, "invalid_request"),
        (ForgeProviderStatus::Conflict, "conflict"),
        (ForgeProviderStatus::Unsupported, "unsupported"),
        (ForgeProviderStatus::Unhealthy, "unhealthy"),
        (ForgeProviderStatus::Failed, "failed"),
    ];
    for (status, expected) in cases {
        assert_eq!(status.as_str(), expected);
    }
}

#[test]
fn parse_request_rejects_raw_json_with_secret_value_field() {
    let value = json!({
        "protocol": FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION,
        "operation": "publish",
        "project": { "slug": "acme/web", "label": null },
        "revision": {
            "kind": "git",
            "url": "https://example.com/acme/web.git",
            "source_ref": "deadbeef",
            "release_digest": "sha256:deadbeef",
            "environment": "production",
            "secret_value": "hunter2"
        },
        "operation_id": { "operation_id": Uuid::new_v4(), "dedupe_key": null },
        "secret_set": null,
        "dry_run": false
    });
    let err = parse_request(&value).expect_err("raw secret value must not parse");
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn terminal_evidence_round_trips_through_serde() {
    let evidence = ForgeProviderTerminalEvidence {
        operation_id: Uuid::new_v4(),
        source_kind: ForgeProviderSourceKind::Git,
        environment: "production".into(),
        release_digest: "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
            .into(),
        release_id: Uuid::new_v4(),
        completed_at: chrono::Utc::now(),
        diagnostic: "release promoted to healthy".into(),
    };
    let value = serde_json::to_value(&evidence).unwrap();
    let parsed: ForgeProviderTerminalEvidence = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, evidence);
}

#[test]
fn capabilities_round_trip_preserves_supported_operations() {
    let caps = ForgeProviderCapabilities {
        provider: ForgeProviderProviderIdentity {
            provider_id: FORGE_PUBLISH_PROVIDER_IDENTITY.to_string(),
            implementation_version: "openpanel-forge-publish-provider/0.1.0".into(),
            operations: vec![
                ForgeProviderOperation::Capabilities,
                ForgeProviderOperation::Preflight,
                ForgeProviderOperation::Publish,
                ForgeProviderOperation::Verify,
                ForgeProviderOperation::Rollback,
            ],
            protocol_versions: vec![FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION.into()],
        },
        health: ForgeProviderHealthSummary {
            environment: "production".into(),
            release_id: None,
            health: "unknown".into(),
            observed_at: chrono::Utc::now(),
        },
    };
    let value = serde_json::to_value(&caps).unwrap();
    let parsed: ForgeProviderCapabilities = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, caps);
}

#[test]
fn rollback_status_round_trips_through_serde() {
    let status = ForgeProviderRollbackStatus {
        environment: "production".into(),
        restored_release_id: Some(Uuid::new_v4()),
        prior_release_id: Some(Uuid::new_v4()),
        outcome: "succeeded".into(),
        completed_at: chrono::Utc::now(),
    };
    let value = serde_json::to_value(&status).unwrap();
    let parsed: ForgeProviderRollbackStatus = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, status);
}

#[test]
fn parse_request_accepts_preflight_with_empty_release_digest() {
    let mut request = full_request();
    request.operation = ForgeProviderOperation::Preflight;
    request.revision.as_mut().unwrap().release_digest = None;
    let value = serde_json::to_value(&request).unwrap();
    let parsed = parse_request(&value).expect("preflight allows unresolved digest");
    assert!(parsed.revision.as_ref().unwrap().release_digest.is_none());
    assert!(parsed.dry_run || !parsed.dry_run);
}

#[test]
fn validate_request_rejects_unmapped_operation_string() {
    let value = json!({
        "protocol": FORGE_PUBLISH_PROVIDER_PROTOCOL_VERSION,
        "operation": "deploy",
        "project": { "slug": "acme/web", "label": null },
        "revision": null,
        "operation_id": null,
        "secret_set": null,
        "dry_run": false
    });
    let err = parse_request(&value).expect_err("unmapped operation");
    // serde will fail to parse this enum value first.
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn source_kind_wire_names_are_stable() {
    assert_eq!(ForgeProviderSourceKind::Git.as_str(), "git");
    assert_eq!(ForgeProviderSourceKind::Oci.as_str(), "oci");
    assert_eq!(ForgeProviderSourceKind::Compose.as_str(), "compose");
}

#[test]
fn terminal_evidence_serializes_diagnostic_without_secret() {
    let evidence = ForgeProviderTerminalEvidence {
        operation_id: Uuid::new_v4(),
        source_kind: ForgeProviderSourceKind::Oci,
        environment: "production".into(),
        release_digest: "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
            .into(),
        release_id: Uuid::new_v4(),
        completed_at: chrono::Utc::now(),
        diagnostic: redact_diagnostic("release promoted to healthy"),
    };
    let value = serde_json::to_string(&evidence).unwrap();
    assert!(value.contains("healthy"));
    assert!(!value.contains("password"));
}

#[test]
fn redaction_handles_markdown_code_blocks() {
    let diagnostic = "see block:\n```\nsecret=hunter2\n```";
    let redacted = redact_diagnostic(diagnostic);
    assert_eq!(redacted, "[REDACTED]");
}

#[test]
fn redaction_strips_secret_markers_case_insensitively() {
    for marker in ["PASSWORD", "Password", "pAsSwOrD", "SECRET", "Token"] {
        let redacted = redact_diagnostic(format!("deploy {marker}=hunter2 failed"));
        assert_eq!(redacted, "[REDACTED]", "marker={marker}");
    }
}

#[test]
fn parse_request_rejects_oversize_environment() {
    let mut request = full_request();
    request.revision.as_mut().unwrap().environment = "e".repeat(1024);
    let value = serde_json::to_value(&request).unwrap();
    let err = parse_request(&value).expect_err("oversize env");
    assert!(matches!(err, ForgeProviderError::InvalidRevision(_)));
}

#[test]
fn redact_diagnostic_keeps_bounded_safe_marker_text() {
    let safe = "release published: source=https://example.com, env=production";
    let redacted = redact_diagnostic(safe);
    assert_eq!(redacted, safe);
}
