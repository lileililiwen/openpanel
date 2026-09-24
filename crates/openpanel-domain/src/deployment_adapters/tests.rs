//! Deployment-adapter unit + property tests. Covers the
//! capability marker, action classification, manifest
//! validation, plan/evidence invariants, idempotency
//! decisions, diagnostic redaction, and the bounded
//! identifier length.

#[cfg(test)]
mod tests {
    // `mod tests` inside `tests.rs` is the conventional Rust
    // pattern for organizing unit + property tests. Suppress
    // `module_inception` so the file name and the inner
    // module name stay self-documenting.
    #![allow(clippy::module_inception)]
    // use super::*; // replaced by explicit imports below
    use crate::deployment_adapters::logic::{
        IdempotencyDecision, decide_replay, operation_record_id, validate_plan,
    };
    use crate::deployment_adapters::types::{
        AdapterManifest, DeploymentAction, DeploymentAdapterError, DeploymentEvidence,
        DeploymentPlan, DeploymentState, HealthMethod, MAX_DIAGNOSTIC_LEN, OperationKey,
        RUNTIME_CONTRACT_VERSION, RollbackPolicy, SecretRef, redact_diagnostic,
    };
    use chrono::{DateTime, TimeZone, Utc};

    /// Capability under test: `deployment-adapters`.
    const CAPABILITY: &str = "deployment-adapters";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 24, 0, 0, 0)
            .single()
            .expect("test time")
    }

    fn sample_manifest() -> AdapterManifest {
        AdapterManifest::new(
            "oci-compose-v1",
            "Compose on the OCI image",
            RUNTIME_CONTRACT_VERSION,
            "oci://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Rollback,
            ],
            true,
            HealthMethod::HttpGet {
                url: "http://127.0.0.1:8080/healthz".into(),
                expect_body_contains: Some("ok".into()),
            },
            vec!["agent-token".into()],
        )
        .expect("manifest")
    }

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "deployment-adapters");
    }

    #[test]
    fn action_classification_is_mutating_for_deploy_restart_rollback_preflight() {
        assert!(DeploymentAction::Deploy.is_mutating());
        assert!(DeploymentAction::Restart.is_mutating());
        assert!(DeploymentAction::Rollback.is_mutating());
        assert!(DeploymentAction::Preflight.is_mutating());
        assert!(!DeploymentAction::Status.is_mutating());
        assert!(!DeploymentAction::Verify.is_mutating());
        assert!(!DeploymentAction::Logs.is_mutating());
    }

    #[test]
    fn action_rollback_policy_is_attempt_only_for_deploy_and_restart() {
        assert_eq!(
            DeploymentAction::Deploy.rollback_policy(),
            RollbackPolicy::Attempt
        );
        assert_eq!(
            DeploymentAction::Restart.rollback_policy(),
            RollbackPolicy::Attempt
        );
        assert_eq!(
            DeploymentAction::Rollback.rollback_policy(),
            RollbackPolicy::Skip
        );
        assert_eq!(
            DeploymentAction::Preflight.rollback_policy(),
            RollbackPolicy::Skip
        );
    }

    #[test]
    fn manifest_rejects_empty_or_dup_action_list() {
        let err = AdapterManifest::new(
            "id",
            "label",
            RUNTIME_CONTRACT_VERSION,
            "scheme://",
            vec![],
            false,
            HealthMethod::TcpConnect {
                address: "127.0.0.1:22".into(),
            },
            vec![],
        );
        assert!(matches!(
            err,
            Err(DeploymentAdapterError::IncompleteManifest("actions"))
        ));
        let dup = AdapterManifest::new(
            "id",
            "label",
            RUNTIME_CONTRACT_VERSION,
            "scheme://",
            vec![DeploymentAction::Deploy, DeploymentAction::Deploy],
            false,
            HealthMethod::TcpConnect {
                address: "127.0.0.1:22".into(),
            },
            vec![],
        );
        assert!(matches!(dup, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn manifest_rejects_unsupported_contract_version() {
        let err = AdapterManifest::new(
            "id",
            "label",
            RUNTIME_CONTRACT_VERSION + 1,
            "scheme://",
            vec![DeploymentAction::Status],
            false,
            HealthMethod::TcpConnect {
                address: "127.0.0.1:22".into(),
            },
            vec![],
        );
        assert!(matches!(
            err,
            Err(DeploymentAdapterError::IncompleteManifest(
                "contract_version"
            ))
        ));
    }

    #[test]
    fn manifest_supports_lookup_is_binary() {
        let manifest = sample_manifest();
        assert!(manifest.supports(DeploymentAction::Deploy));
        assert!(manifest.supports(DeploymentAction::Rollback));
        assert!(!manifest.supports(DeploymentAction::Logs));
    }

    #[test]
    fn plan_rejects_mutating_action_without_operation_key() {
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            None,
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        );
        assert!(plan.is_err());
    }

    #[test]
    fn plan_rejects_read_only_action_with_operation_key() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Status, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Status,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        );
        assert!(plan.is_err());
    }

    #[test]
    fn validate_plan_refuses_undeclared_action() {
        let manifest = sample_manifest();
        // `sample_manifest` does not declare `Logs`; the plan MUST
        // omit an operation key (Logs is read-only).
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Logs,
            None,
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(
            result,
            Err(DeploymentAdapterError::UnsupportedAction("logs"))
        ));
    }

    #[test]
    fn validate_plan_refuses_undeclared_secret_reference() {
        let manifest = sample_manifest();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let secret = SecretRef::new("not-declared", None).unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![secret],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(result, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn validate_plan_refuses_rollback_when_manifest_does_not_declare_it() {
        // The manifest declares `Rollback` as a supported action but
        // does NOT declare rollback support (the boolean). The
        // rollback-only check fires after the action-declared check,
        // so we must include Rollback in the action list to reach it.
        let manifest = AdapterManifest::new(
            "oci-compose-v1",
            "Compose on the OCI image",
            RUNTIME_CONTRACT_VERSION,
            "oci://",
            vec![
                DeploymentAction::Preflight,
                DeploymentAction::Deploy,
                DeploymentAction::Verify,
                DeploymentAction::Status,
                DeploymentAction::Rollback,
            ],
            false,
            HealthMethod::HttpGet {
                url: "http://127.0.0.1:8080/healthz".into(),
                expect_body_contains: Some("ok".into()),
            },
            vec!["agent-token".into()],
        )
        .unwrap();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Rollback, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Rollback,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(result, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn validate_plan_refuses_adapter_mismatch() {
        let manifest = sample_manifest();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "mac-jenkins-v1",
            "owner",
            false,
        )
        .unwrap();
        let result = validate_plan(&plan, &manifest);
        assert!(matches!(result, Err(DeploymentAdapterError::Invalid(_))));
    }

    #[test]
    fn validate_plan_accepts_dry_run_deploy() {
        let manifest = sample_manifest();
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            true,
        )
        .unwrap();
        assert_eq!(
            validate_plan(&plan, &manifest).unwrap(),
            RollbackPolicy::Attempt
        );
    }

    #[test]
    fn operation_record_id_is_deterministic_for_same_key() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan_a = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key.clone()),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let plan_b = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        assert_eq!(
            operation_record_id(&plan_a).unwrap(),
            operation_record_id(&plan_b).unwrap()
        );
    }

    #[test]
    fn decide_replay_returns_replay_for_terminal_matching_evidence() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let cached = DeploymentEvidence::new(
            "oci://web-01",
            "oci-compose-v1",
            DeploymentAction::Deploy,
            plan.operation_key().cloned(),
            "sha256:deadbeef",
            DeploymentState::Ready,
            now(),
            now() + chrono::Duration::seconds(5),
            "ok",
        )
        .unwrap();
        assert_eq!(
            decide_replay(&plan, Some(&cached)),
            IdempotencyDecision::Replay
        );
    }

    #[test]
    fn decide_replay_returns_rerun_when_release_digest_differs() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let cached = DeploymentEvidence::new(
            "oci://web-01",
            "oci-compose-v1",
            DeploymentAction::Deploy,
            plan.operation_key().cloned(),
            "sha256:otherdigest",
            DeploymentState::Ready,
            now(),
            now() + chrono::Duration::seconds(5),
            "ok",
        )
        .unwrap();
        assert_eq!(
            decide_replay(&plan, Some(&cached)),
            IdempotencyDecision::Rerun
        );
    }

    #[test]
    fn decide_replay_returns_rerun_when_evidence_not_terminal() {
        let key = OperationKey::new("oci://web-01", DeploymentAction::Deploy, "v1").unwrap();
        let plan = DeploymentPlan::new(
            "oci://web-01",
            DeploymentAction::Deploy,
            Some(key),
            "sha256:deadbeef",
            vec![],
            "oci-compose-v1",
            "owner",
            false,
        )
        .unwrap();
        let cached = DeploymentEvidence::new(
            "oci://web-01",
            "oci-compose-v1",
            DeploymentAction::Deploy,
            plan.operation_key().cloned(),
            "sha256:deadbeef",
            DeploymentState::Verifying,
            now(),
            now() + chrono::Duration::seconds(5),
            "still running",
        )
        .unwrap();
        assert_eq!(
            decide_replay(&plan, Some(&cached)),
            IdempotencyDecision::Rerun
        );
    }

    #[test]
    fn redact_diagnostic_strips_password_token_and_pem() {
        let raw = "preflight password=hunter2 ok\nbearer abcdef0123456789abcdef0123456789\n-----BEGIN RSA PRIVATE KEY-----\nxxx\n-----END RSA PRIVATE KEY-----";
        let redacted = redact_diagnostic(raw);
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("abcdef0123456789"));
        assert!(!redacted.contains("BEGIN RSA PRIVATE KEY"));
        assert!(redacted.contains("preflight"));
    }

    #[test]
    fn redact_diagnostic_truncates_to_max_diagnostic_len() {
        let raw: String = "a".repeat(MAX_DIAGNOSTIC_LEN * 2);
        let redacted = redact_diagnostic(&raw);
        assert!(redacted.len() <= MAX_DIAGNOSTIC_LEN);
    }

    #[test]
    fn state_terminal_and_failure_classification() {
        assert!(DeploymentState::Ready.is_terminal());
        assert!(DeploymentState::Ready.is_ready());
        assert!(!DeploymentState::Ready.is_failure());
        assert!(DeploymentState::Failed.is_terminal());
        assert!(DeploymentState::Failed.is_failure());
        assert!(DeploymentState::RollbackFailed.is_terminal());
        assert!(DeploymentState::RollbackFailed.is_failure());
        assert!(!DeploymentState::Applied.is_terminal());
        assert!(!DeploymentState::Verifying.is_terminal());
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_operation_record_id_is_stable(
                target in "[a-z0-9:/\\-]{1,16}",
                value in "[a-z0-9]{1,16}",
            ) {
                let key = OperationKey::new(&target, DeploymentAction::Deploy, &value).unwrap();
                let plan = DeploymentPlan::new(
                    &target,
                    DeploymentAction::Deploy,
                    Some(key),
                    "sha256:abc",
                    vec![],
                    "oci-compose-v1",
                    "owner",
                    false,
                )
                .unwrap();
                let first = operation_record_id(&plan).unwrap();
                let second = operation_record_id(&plan).unwrap();
                prop_assert_eq!(first, second);
            }

            #[test]
            fn prop_redact_never_keeps_password_marker(line in ".{1,80}") {
                let with = format!("password=hunter2 {line}");
                let redacted = redact_diagnostic(&with);
                prop_assert!(!redacted.contains("hunter2"));
            }
        }
    }
}
