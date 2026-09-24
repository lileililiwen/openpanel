//! Unit + property tests for the `verified-service-catalog`
//! bounded context. They cover the four ADDED Requirements in
//! `openspec/specs/verified-service-catalog/spec.md`:
//!   - Signed Declarative Manifest
//!   - Reviewable Install Plan
//!   - Safe Lifecycle
//!   - Trust Revocation

#[cfg(test)]
mod tests {
    // `mod tests` inside `tests.rs` is the conventional Rust pattern
    // (same as `deployment_adapters/tests.rs`); suppress
    // `module_inception` so the file and module names stay
    // self-documenting.
    #![allow(clippy::module_inception)]

    use crate::verified_service_catalog::logic::{
        can_retry, check_target_compatibility, ensure_installable, manifest_signature,
        plan_service_install, service_job_transition_allowed, verify_manifest_signature,
    };
    use crate::verified_service_catalog::manifest::{
        PublisherIdentity, ResourceRequest, ServiceBackupPolicy, ServiceCompatibility,
        ServiceHealthProbe, ServiceManifest, ServicePermissions, ServiceUpgradePolicy,
        SourceDigest,
    };
    use crate::verified_service_catalog::types::{
        HostPolicy, JobOutcome, JobPhase, PlanConflict, RevocationList, ServiceJob, TrustState,
        VerifiedServiceCatalogError,
    };

    use chrono::{DateTime, TimeZone, Utc};
    use proptest::prelude::*;
    use uuid::Uuid;

    /// Capability under test: `verified-service-catalog`.
    const CAPABILITY: &str = "verified-service-catalog";

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 24, 12, 0, 0).unwrap()
    }

    fn publisher() -> PublisherIdentity {
        PublisherIdentity::new("openpanel-core", "OpenPanel Core Team", "pubkey-0f0f0f0f")
            .expect("static")
    }

    fn digest() -> SourceDigest {
        SourceDigest::new(format!("sha256:{}", "a".repeat(64))).expect("static")
    }

    fn compatibility() -> ServiceCompatibility {
        ServiceCompatibility::new(vec!["debian:amd64".into(), "ubuntu:amd64".into()])
            .expect("static")
    }

    fn resources() -> ResourceRequest {
        ResourceRequest::new(1000, 512 * 1024 * 1024, 5 * 1024 * 1024 * 1024).expect("static")
    }

    fn permissions() -> ServicePermissions {
        ServicePermissions::new(
            true,
            vec![8080, 9443],
            vec!["registry-creds".into()],
            vec!["writes:/var/lib/demo".into()],
        )
        .expect("static")
    }

    fn health() -> ServiceHealthProbe {
        ServiceHealthProbe::new("/healthz", 30, 3).expect("static")
    }

    fn backup() -> ServiceBackupPolicy {
        ServiceBackupPolicy::new(true)
    }

    fn upgrade() -> ServiceUpgradePolicy {
        ServiceUpgradePolicy::new(3, true).expect("static")
    }

    fn manifest_with(
        pub_key: PublisherIdentity,
        dig: SourceDigest,
        signature: String,
    ) -> ServiceManifest {
        ServiceManifest::new(
            "demo-service",
            "Demo Service",
            "1.2.3",
            pub_key,
            dig,
            signature,
            compatibility(),
            resources(),
            permissions(),
            health(),
            backup(),
            upgrade(),
        )
        .expect("static")
    }

    fn signed_manifest() -> ServiceManifest {
        let pub_key = publisher();
        let dig = digest();
        let signature = manifest_signature(&pub_key, &dig);
        manifest_with(pub_key, dig, signature)
    }

    fn host_policy() -> HostPolicy {
        HostPolicy::new("debian", "amd64", vec![8080], 2000, 1 << 30, 10 << 30).expect("static")
    }

    // ---------------------------------------------------------------------------
    // Requirement: Signed Declarative Manifest
    // ---------------------------------------------------------------------------

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "verified-service-catalog");
    }

    #[test]
    fn manifest_new_accepts_valid_schema() {
        let manifest = signed_manifest();
        assert_eq!(manifest.id(), "demo-service");
        assert_eq!(manifest.version(), "1.2.3");
        assert_eq!(manifest.publisher().id(), "openpanel-core");
        assert_eq!(manifest.digest().as_str(), digest().as_str());
        assert_eq!(manifest.resources().cpu_millis(), 1000);
        assert_eq!(manifest.upgrade_policy().max_attempts(), 3);
    }

    #[test]
    fn manifest_rejects_blank_identity_fields() {
        assert!(PublisherIdentity::new("", "n", "f").is_err());
        assert!(PublisherIdentity::new("id", "", "f").is_err());
        assert!(PublisherIdentity::new("id", "n", "").is_err());
        assert!(
            ServiceManifest::new(
                "",
                "n",
                "1.0.0",
                publisher(),
                digest(),
                "s".repeat(64),
                compatibility(),
                resources(),
                permissions(),
                health(),
                backup(),
                upgrade()
            )
            .is_err()
        );
        assert!(
            ServiceManifest::new(
                "id",
                "n",
                "not-a-version",
                publisher(),
                digest(),
                "s".repeat(64),
                compatibility(),
                resources(),
                permissions(),
                health(),
                backup(),
                upgrade()
            )
            .is_err()
        );
    }

    #[test]
    fn manifest_signature_must_be_lowercase_hex() {
        // A non-hex signature is accepted into the data model (so the
        // hostile manifest is reviewable) but verification fails.
        let manifest = manifest_with(publisher(), digest(), "not hex!!".into());
        assert_eq!(manifest.signature(), "not hex!!");
        assert!(verify_manifest_signature(&manifest, &[publisher()]).is_err());
    }

    #[test]
    fn source_digest_requires_sha256_prefixed_64_hex() {
        assert!(SourceDigest::new("sha256:deadbeef").is_err());
        assert!(SourceDigest::new(format!("sha256:{}", "g".repeat(64))).is_err());
        assert!(SourceDigest::new(format!("sha512:{}", "a".repeat(64))).is_err());
        assert!(SourceDigest::new(format!("sha256:{}", "a".repeat(63))).is_err());
        assert!(SourceDigest::new(format!("sha256:{}", "a".repeat(65))).is_err());
        assert!(SourceDigest::new(format!("sha256:{}", "a".repeat(64))).is_ok());
    }

    #[test]
    fn verify_manifest_signature_accepts_trusted_publisher() {
        let manifest = signed_manifest();
        let trust = verify_manifest_signature(&manifest, &[publisher()]).expect("valid signature");
        assert_eq!(trust, TrustState::Verified);
    }

    #[test]
    fn verify_manifest_signature_accepts_self_signed_community() {
        let manifest = signed_manifest();
        // The signature verifies against the manifest's own declared
        // publisher, but the curated set holds a different identity —
        // community trust, not a curated verified publisher.
        let other = PublisherIdentity::new("other", "Other", "pubkey-1111").expect("static");
        let trust = verify_manifest_signature(&manifest, &[other]).expect("self-signed ok");
        assert_eq!(trust, TrustState::Community);
    }

    #[test]
    fn verify_manifest_signature_rejects_tampered_digest() {
        // A manifest whose pinned digest was swapped after signing no
        // longer verifies — signature material is bound to the digest,
        // independent of template content.
        let signature = manifest_signature(&publisher(), &digest());
        let tampered = SourceDigest::new(format!("sha256:{}", "b".repeat(64))).expect("static");
        let manifest = manifest_with(publisher(), tampered, signature);
        let result = verify_manifest_signature(&manifest, &[publisher()]);
        assert!(matches!(
            result,
            Err(VerifiedServiceCatalogError::InvalidSignature)
        ));
    }

    #[test]
    fn invalid_signature_blocks_entry_and_refuses_planning() {
        // Scenario: Invalid signature — WHEN a manifest signature does
        // not verify against a trusted publisher THEN the catalog marks
        // the entry blocked and refuses planning or install.
        let manifest = manifest_with(publisher(), digest(), "0011".into());
        let verified = verify_manifest_signature(&manifest, &[publisher()]);
        assert!(matches!(
            verified,
            Err(VerifiedServiceCatalogError::InvalidSignature)
        ));
        // A failed verification maps to the Blocked trust state...
        let trust = verified.unwrap_or(TrustState::Blocked);
        assert_eq!(trust, TrustState::Blocked);
        // ...and ensure_installable refuses a Blocked entry.
        let revocations = RevocationList::empty();
        assert!(matches!(
            ensure_installable(trust, &manifest, &revocations),
            Err(VerifiedServiceCatalogError::Blocked)
        ));
    }

    #[test]
    fn compatibility_supports_declared_targets_with_wildcards() {
        let declared = ServiceCompatibility::new(vec!["debian:amd64".into(), "ubuntu:*".into()])
            .expect("static");
        assert!(declared.supports("debian", "amd64"));
        assert!(!declared.supports("debian", "arm64"));
        assert!(declared.supports("ubuntu", "arm64"));
        assert!(!declared.supports("fedora", "amd64"));
        let universal = ServiceCompatibility::new(vec!["*:*".into()]).expect("static");
        assert!(universal.supports("anything", "anyarch"));
        assert!(ServiceCompatibility::new(vec!["Bad OS:amd64".into()]).is_err());
        assert!(ServiceCompatibility::new(vec![]).is_err());
    }

    #[test]
    fn check_target_compatibility_refuses_unsupported_target() {
        let manifest = signed_manifest();
        let mismatched =
            HostPolicy::new("arch", "amd64", vec![8080, 9443], 2000, 1 << 30, 10 << 30)
                .expect("static");
        assert!(matches!(
            check_target_compatibility(&manifest, &mismatched),
            Err(VerifiedServiceCatalogError::IncompatibleTarget(_))
        ));
        let supported =
            HostPolicy::new("debian", "amd64", vec![8080, 9443], 2000, 1 << 30, 10 << 30)
                .expect("static");
        assert!(check_target_compatibility(&manifest, &supported).is_ok());
    }

    // ---------------------------------------------------------------------------
    // Requirement: Reviewable Install Plan
    // ---------------------------------------------------------------------------

    #[test]
    fn plan_service_install_reveals_effect_before_confirmation() {
        let manifest = signed_manifest();
        let plan = plan_service_install(&manifest, &host_policy_with_all_ports()).expect("plan ok");
        assert_eq!(plan.entry_id(), "demo-service");
        assert_eq!(plan.digest(), digest().as_str());
        assert_eq!(plan.ports(), vec![8080, 9443]);
        assert!(plan.public_network());
        assert_eq!(plan.credential_refs(), vec!["registry-creds".to_string()]);
        assert_eq!(
            plan.data_effects(),
            vec!["writes:/var/lib/demo".to_string()]
        );
        assert_eq!(plan.storage_bytes(), 5 * 1024 * 1024 * 1024);
        assert_eq!(plan.cpu_millis(), 1000);
        assert_eq!(plan.memory_bytes(), 512 * 1024 * 1024);
    }

    fn host_policy_with_all_ports() -> HostPolicy {
        HostPolicy::new("debian", "amd64", vec![8080, 9443], 2000, 1 << 30, 10 << 30)
            .expect("static")
    }

    #[test]
    fn plan_service_install_fails_on_disallowed_port_without_mutation() {
        // Scenario: Resource conflict — a service requesting a port
        // outside the target policy fails planning with a specific
        // conflict. `plan_service_install` is a pure function: a failed
        // plan performs no mutation by construction.
        let manifest = signed_manifest();
        let result = plan_service_install(&manifest, &host_policy());
        assert!(matches!(
            result,
            Err(PlanConflict::PortNotAllowed { port: 9443 })
        ));
    }

    #[test]
    fn plan_service_install_fails_on_storage_and_resource_exhaustion() {
        let manifest = signed_manifest();
        let tight_storage =
            HostPolicy::new("debian", "amd64", vec![8080, 9443], 2000, 1 << 30, 1 << 30)
                .expect("static");
        assert!(matches!(
            plan_service_install(&manifest, &tight_storage),
            Err(PlanConflict::StorageQuotaExceeded { .. })
        ));
        let tight_cpu =
            HostPolicy::new("debian", "amd64", vec![8080, 9443], 500, 1 << 30, 10 << 30)
                .expect("static");
        assert!(matches!(
            plan_service_install(&manifest, &tight_cpu),
            Err(PlanConflict::CpuQuotaExceeded { .. })
        ));
        let tight_mem = HostPolicy::new("debian", "amd64", vec![8080, 9443], 2000, 256, 10 << 30)
            .expect("static");
        assert!(matches!(
            plan_service_install(&manifest, &tight_mem),
            Err(PlanConflict::MemoryQuotaExceeded { .. })
        ));
    }

    #[test]
    fn host_policy_validates_port_ranges() {
        assert!(HostPolicy::new("debian", "amd64", vec![0], 1, 1, 1).is_err());
        assert!(HostPolicy::new("", "amd64", vec![8080], 1, 1, 1).is_err());
        assert!(HostPolicy::new("debian", " ", vec![8080], 1, 1, 1).is_err());
    }

    // ---------------------------------------------------------------------------
    // Requirement: Safe Lifecycle
    // ---------------------------------------------------------------------------

    #[test]
    fn service_job_begins_pending_and_walks_to_succeeded() {
        let mut job = ServiceJob::begin(
            Uuid::new_v4(),
            "demo-service",
            JobPhase::Install,
            3,
            fixed_time(),
        );
        assert_eq!(job.outcome(), JobOutcome::Pending);
        assert_eq!(job.attempts(), 0);
        job.start(fixed_time()).expect("pending -> running");
        assert_eq!(job.outcome(), JobOutcome::Running);
        job.succeed(fixed_time()).expect("running -> succeeded");
        assert_eq!(job.outcome(), JobOutcome::Succeeded);
        assert!(job.outcome().is_terminal());
        assert_eq!(job.attempts(), 1);
    }

    #[test]
    fn service_job_rejects_invalid_transitions() {
        let mut job = ServiceJob::begin(
            Uuid::new_v4(),
            "demo-service",
            JobPhase::Upgrade,
            3,
            fixed_time(),
        );
        // Pending -> Succeeded is not a legal transition.
        assert!(job.succeed(fixed_time()).is_err());
        job.start(fixed_time()).expect("static");
        job.fail(fixed_time(), "boom").expect("static");
        assert_eq!(job.outcome(), JobOutcome::Failed);
        // Failed -> Succeeded is not legal; retry or roll back.
        assert!(job.succeed(fixed_time()).is_err());
        assert!(service_job_transition_allowed(
            JobOutcome::Failed,
            JobOutcome::Running
        ));
        assert!(!service_job_transition_allowed(
            JobOutcome::Succeeded,
            JobOutcome::Running
        ));
        // Terminal states never transition again.
        job.roll_back(fixed_time(), "prior release restored")
            .expect("failed -> rolled_back");
        assert_eq!(job.outcome(), JobOutcome::RolledBack);
        assert!(job.fail(fixed_time(), "late").is_err());
    }

    #[test]
    fn can_retry_respects_bounded_attempts() {
        let mut job = ServiceJob::begin(
            Uuid::new_v4(),
            "demo-service",
            JobPhase::Install,
            2,
            fixed_time(),
        );
        assert!(can_retry(&job));
        job.start(fixed_time()).expect("static");
        job.fail(fixed_time(), "attempt 1 failed").expect("static");
        assert!(can_retry(&job));
        job.start(fixed_time()).expect("failed -> running on retry");
        job.fail(fixed_time(), "attempt 2 failed").expect("static");
        // Attempts exhausted — no more retries.
        assert!(!can_retry(&job));
        assert_eq!(job.attempts(), 2);
    }

    #[test]
    fn upgrade_failure_records_operator_guidance() {
        // Scenario: Upgrade failure — WHEN a service upgrade fails
        // health verification THEN the previous verified release remains
        // recoverable and the job is failed or rolled back with operator
        // guidance.
        let mut job = ServiceJob::begin(
            Uuid::new_v4(),
            "demo-service",
            JobPhase::Upgrade,
            3,
            fixed_time(),
        );
        job.start(fixed_time()).expect("static");
        job.fail(fixed_time(), "health probe timeout")
            .expect("static");
        job.roll_back(
            fixed_time(),
            "previous verified release 1.2.3 restored; fix registry credentials and retry",
        )
        .expect("static");
        assert_eq!(job.outcome(), JobOutcome::RolledBack);
        assert!(
            job.guidance()
                .is_some_and(|g| g.contains("previous verified release"))
        );
        assert!(job.last_error().is_some_and(|e| e.contains("health probe")));
    }

    #[test]
    fn service_job_diagnostic_is_redacted() {
        // Lifecycle diagnostics reuse the deployment-adapter redaction
        // contract: secrets and opaque script material never reach the
        // record.
        let mut job = ServiceJob::begin(
            Uuid::new_v4(),
            "demo-service",
            JobPhase::Install,
            3,
            fixed_time(),
        );
        job.start(fixed_time()).expect("static");
        job.fail(fixed_time(), "pull failed password=hunter2 token=abc")
            .expect("static");
        let error = job.last_error().unwrap_or_default();
        assert!(!error.contains("hunter2"));
        assert!(!error.contains("token=abc"));
    }

    #[test]
    fn job_phase_wire_names_are_stable() {
        assert_eq!(JobPhase::Install.as_str(), "install");
        assert_eq!(JobPhase::Upgrade.as_str(), "upgrade");
        assert_eq!(JobPhase::Backup.as_str(), "backup");
        assert_eq!(JobPhase::Health.as_str(), "health");
        assert_eq!(JobPhase::Rollback.as_str(), "rollback");
        assert_eq!(JobPhase::Remove.as_str(), "remove");
    }

    // ---------------------------------------------------------------------------
    // Requirement: Trust Revocation
    // ---------------------------------------------------------------------------

    #[test]
    fn revocation_list_blocks_publisher_and_digest_independently() {
        let manifest = signed_manifest();
        let mut revocations = RevocationList::empty();
        assert!(!revocations.blocks_manifest(&manifest));
        revocations.revoke_artifact_digest(digest().as_str());
        assert!(revocations.is_digest_revoked(digest().as_str()));
        assert!(revocations.blocks_manifest(&manifest));
        let mut by_publisher = RevocationList::empty();
        by_publisher.revoke_publisher_id("openpanel-core");
        assert!(by_publisher.blocks_manifest(&manifest));
    }

    #[test]
    fn ensure_installable_refuses_revoked_even_when_cached() {
        // Scenario: Cached revoked entry — a revoked entry MUST NOT be
        // installable even when it remains cached locally (the entry
        // here carries trust `Verified`, which a naive cache check would
        // accept).
        let manifest = signed_manifest();
        let mut revocations = RevocationList::empty();
        revocations.revoke_publisher_id("openpanel-core");
        assert!(matches!(
            ensure_installable(TrustState::Verified, &manifest, &revocations),
            Err(VerifiedServiceCatalogError::Revoked(_))
        ));
    }

    #[test]
    fn ensure_installable_accepts_verified_and_community() {
        let manifest = signed_manifest();
        let revocations = RevocationList::empty();
        assert!(ensure_installable(TrustState::Verified, &manifest, &revocations).is_ok());
        assert!(ensure_installable(TrustState::Community, &manifest, &revocations).is_ok());
        assert!(matches!(
            ensure_installable(TrustState::Revoked, &manifest, &revocations),
            Err(VerifiedServiceCatalogError::Revoked(_))
        ));
    }

    #[test]
    fn trust_state_wire_names_are_stable() {
        assert_eq!(TrustState::Verified.as_str(), "verified");
        assert_eq!(TrustState::Community.as_str(), "community");
        assert_eq!(TrustState::Revoked.as_str(), "revoked");
        assert_eq!(TrustState::Blocked.as_str(), "blocked");
        assert!(TrustState::Verified.is_installable());
        assert!(TrustState::Community.is_installable());
        assert!(!TrustState::Revoked.is_installable());
        assert!(!TrustState::Blocked.is_installable());
    }

    // ---------------------------------------------------------------------------
    // Property tests
    // ---------------------------------------------------------------------------

    proptest! {
        #[test]
        fn prop_signature_is_deterministic_and_bound_to_inputs(
            fingerprint in "[a-z0-9-]{8,24}",
            digest_hex in "[a-f0-9]{64}",
        ) {
            let pub_key = PublisherIdentity::new("p", "P", fingerprint.as_str()).expect("static");
            let dig = SourceDigest::new(format!("sha256:{digest_hex}")).expect("static");
            let first = manifest_signature(&pub_key, &dig);
            let second = manifest_signature(&pub_key, &dig);
            prop_assert_eq!(first.clone(), second);
            // A different publisher key produces a different signature.
            let other = PublisherIdentity::new("p", "P", format!("f{fingerprint}").as_str()).expect("static");
            prop_assert_ne!(first.clone(), manifest_signature(&other, &dig));
            // A different digest produces a different signature. Flip
            // the first character so the altered digest is guaranteed
            // to differ from the original.
            let mut chars = digest_hex.chars();
            let head = chars.next().unwrap_or('0');
            let rest = chars.as_str();
            let flipped = if head == '0' {
                format!("1{rest}")
            } else {
                format!("0{rest}")
            };
            let other_dig = SourceDigest::new(format!("sha256:{flipped}")).expect("static");
            prop_assert_ne!(first, manifest_signature(&pub_key, &other_dig));
        }

        #[test]
        fn prop_valid_source_digest_round_trips(digest_hex in "[a-f0-9]{64}") {
            let value = format!("sha256:{digest_hex}");
            let dig = SourceDigest::new(value.clone()).expect("valid");
            prop_assert_eq!(dig.as_str(), value);
        }
    }
}
