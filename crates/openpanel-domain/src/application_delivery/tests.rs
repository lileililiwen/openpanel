//! Unit and property tests for the application-delivery bounded
//! context. The file is `#[cfg(test)]`-only; production code
//! never compiles it.

#[cfg(test)]
mod tests {
    // `mod tests` inside `tests.rs` is the conventional Rust
    // pattern for organizing unit + property tests. Suppress
    // `module_inception` so the file name and the inner
    // module name stay self-documenting.
    #![allow(clippy::module_inception)]
    use crate::application_delivery::{
        Application, ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind,
        Environment, EnvironmentKind, PromotionRequest, Release, ReleaseDigest, ReleaseLifecycle,
        ReleaseState, RollbackRequest, SecretSetRef, build_release_identity, can_promote,
        can_rollback, is_mutable_digest, redact_diagnostic, resolve_environment,
        validate_environment_isolation, validate_source_ref,
    };
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    fn now() -> chrono::DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn test_runtime_and_registry_errors_are_typed_and_redacted() {
        assert_eq!(
            ApplicationDeliveryError::RuntimeUnavailable.to_string(),
            "docker runtime unavailable"
        );
        assert_eq!(
            ApplicationDeliveryError::RegistryUnauthorized.to_string(),
            "registry authorization rejected"
        );
    }

    fn make_release(state: ReleaseState) -> (Release, Environment) {
        let app_id = Uuid::new_v4();
        let env_id = Uuid::new_v4();
        let source = ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/repo.git",
            "abcdef1234567890",
        )
        .unwrap();
        let digest = ReleaseDigest::new("sha256:deadbeefcafef00d", now()).unwrap();
        let release = Release::new(Uuid::new_v4(), app_id, env_id, digest, source, now(), None);
        let mut release = release;
        // Walk to the requested state via the lifecycle.
        if matches!(
            state,
            ReleaseState::Building
                | ReleaseState::Deploying
                | ReleaseState::Healthy
                | ReleaseState::Failed
                | ReleaseState::RolledBack
                | ReleaseState::RolledBackFailed
        ) {
            ReleaseLifecycle::start_build(&mut release, now()).unwrap();
        }
        if matches!(
            state,
            ReleaseState::Deploying | ReleaseState::Healthy | ReleaseState::Failed
        ) {
            ReleaseLifecycle::finish_build(
                &mut release,
                now(),
                &ReleaseDigest::new("sha256:deadbeefcafef00d", now()).unwrap(),
            )
            .unwrap();
        }
        if state == ReleaseState::Healthy {
            ReleaseLifecycle::mark_healthy(&mut release, now(), "200 ok").unwrap();
        }
        if matches!(
            state,
            ReleaseState::Failed | ReleaseState::RolledBack | ReleaseState::RolledBackFailed
        ) {
            ReleaseLifecycle::fail(&mut release, now(), "build failed").unwrap();
        }
        if state == ReleaseState::RolledBack {
            ReleaseLifecycle::roll_back(&mut release, now(), "ok").unwrap();
        }
        let prod_secret = SecretSetRef::new("prod-secrets", Some("v1".into())).unwrap();
        let env = Environment::new(
            env_id,
            app_id,
            EnvironmentKind::Production,
            "production",
            4,
            prod_secret,
            now(),
            None,
        )
        .unwrap();
        (release, env)
    }

    #[test]
    fn application_new_rejects_bad_name() {
        let id = Uuid::new_v4();
        let source = ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/repo.git",
            "abcdef1234567890",
        )
        .unwrap();
        let bad = Application::new(id, Uuid::new_v4(), "BadName!", source.clone(), "", now());
        assert!(matches!(bad, Err(ApplicationDeliveryError::Invalid(_))));
        let ok = Application::new(id, Uuid::new_v4(), "web-frontend", source, "", now());
        assert!(ok.is_ok());
    }

    #[test]
    fn application_add_environment_is_sorted_and_dedups() {
        let id = Uuid::new_v4();
        let source = ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/repo.git",
            "abcdef1234567890",
        )
        .unwrap();
        let mut app = Application::new(id, Uuid::new_v4(), "web", source, "", now()).unwrap();
        let prod_secret = SecretSetRef::new("prod-secrets", None).unwrap();
        let preview_secret = SecretSetRef::new("preview-secrets", None).unwrap();
        let prod = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Production,
            "production",
            4,
            prod_secret.clone(),
            now(),
            None,
        )
        .unwrap();
        let preview = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Preview,
            "preview-42",
            2,
            preview_secret,
            now(),
            Some(now() + Duration::hours(2)),
        )
        .unwrap();
        app.add_environment(preview.clone()).unwrap();
        app.add_environment(prod.clone()).unwrap();
        // Sorted by name: preview-42 < production
        assert_eq!(app.environments()[0].name(), "preview-42");
        assert_eq!(app.environments()[1].name(), "production");
        // Duplicate is rejected.
        let again = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Production,
            "production",
            4,
            prod_secret,
            now(),
            None,
        )
        .unwrap();
        assert!(app.add_environment(again).is_err());
        assert!(app.has_production());
    }

    #[test]
    fn environment_requires_kind_specific_secret_prefix() {
        let id = Uuid::new_v4();
        let prod_secret = SecretSetRef::new("prod-secrets", None).unwrap();
        let prod = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Production,
            "production",
            1,
            prod_secret.clone(),
            now(),
            None,
        );
        assert!(prod.is_ok());
        // Production environment with a preview secret set is rejected.
        let preview_secret = SecretSetRef::new("preview-secrets", None).unwrap();
        let wrong = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Production,
            "production",
            1,
            preview_secret,
            now(),
            None,
        );
        assert!(matches!(wrong, Err(ApplicationDeliveryError::Invalid(_))));
        // Preview environment MUST have expires_at and a preview secret set.
        let bad = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Preview,
            "preview-1",
            1,
            prod_secret,
            now(),
            None,
        );
        assert!(matches!(bad, Err(ApplicationDeliveryError::Invalid(_))));
    }

    #[test]
    fn validate_source_ref_rejects_unpinned() {
        // Git SHAs must be 7..=64 hex chars.
        assert!(validate_source_ref(ApplicationSourceKind::Git, "abc").is_err());
        assert!(validate_source_ref(ApplicationSourceKind::Git, "main").is_err());
        assert!(validate_source_ref(ApplicationSourceKind::Git, "abcdef0").is_ok());
        // OCI must be sha256:...
        assert!(validate_source_ref(ApplicationSourceKind::Oci, "latest").is_err());
        assert!(validate_source_ref(ApplicationSourceKind::Oci, "sha256:abcdef0").is_ok());
        // Compose mirrors git.
        assert!(validate_source_ref(ApplicationSourceKind::Compose, "compose").is_err());
        assert!(validate_source_ref(ApplicationSourceKind::Compose, "abcdef0").is_ok());
    }

    #[test]
    fn release_digest_rejects_mutable() {
        let bad = ReleaseDigest::new("latest", now());
        assert!(matches!(
            bad,
            Err(ApplicationDeliveryError::MutableDigest(_))
        ));
        let good = ReleaseDigest::new("sha256:abcdef0", now());
        assert!(good.is_ok());
    }

    #[test]
    fn release_state_machine_walks_full_lifecycle() {
        let (mut release, _env) = make_release(ReleaseState::Pending);
        assert_eq!(release.state(), ReleaseState::Pending);
        ReleaseLifecycle::start_build(&mut release, now()).unwrap();
        assert_eq!(release.state(), ReleaseState::Building);
        ReleaseLifecycle::finish_build(
            &mut release,
            now(),
            &ReleaseDigest::new("sha256:deadbeef", now()).unwrap(),
        )
        .unwrap();
        assert_eq!(release.state(), ReleaseState::Deploying);
        ReleaseLifecycle::mark_healthy(&mut release, now(), "200 ok").unwrap();
        assert_eq!(release.state(), ReleaseState::Healthy);
        assert!(release.state().is_terminal());
        assert!(release.state().is_healthy());
    }

    #[test]
    fn release_state_machine_refuses_illegal_transitions() {
        let (mut release, _env) = make_release(ReleaseState::Pending);
        // Pending -> Deploying is illegal.
        let err = release.transition(ReleaseState::Deploying, now(), "test");
        assert!(matches!(
            err,
            Err(ApplicationDeliveryError::IllegalTransition(_))
        ));
        // Pending -> Healthy is illegal.
        let err = release.transition(ReleaseState::Healthy, now(), "test");
        assert!(matches!(
            err,
            Err(ApplicationDeliveryError::IllegalTransition(_))
        ));
        // Building -> Healthy is illegal (must go through Deploying).
        ReleaseLifecycle::start_build(&mut release, now()).unwrap();
        let err = release.transition(ReleaseState::Healthy, now(), "test");
        assert!(matches!(
            err,
            Err(ApplicationDeliveryError::IllegalTransition(_))
        ));
    }

    #[test]
    fn release_redacts_diagnostic() {
        let (mut release, _env) = make_release(ReleaseState::Building);
        release
            .transition(
                ReleaseState::Failed,
                now(),
                "deploy failed: password=hunter2",
            )
            .unwrap();
        assert!(release.diagnostic().contains("<redacted>"));
        assert!(!release.diagnostic().contains("hunter2"));
    }

    #[test]
    fn promote_requires_matching_release_and_environment() {
        let (release, env) = make_release(ReleaseState::Pending);
        let request = PromotionRequest::new(
            release.application_id(),
            env.id(),
            release.id(),
            "owner",
            "PROMOTE",
            None,
        )
        .unwrap();
        assert!(can_promote(&release, &env, &request).is_ok());
        // Mismatched environment id is rejected.
        let wrong_env = Environment::new(
            Uuid::new_v4(),
            release.application_id(),
            EnvironmentKind::Production,
            "production",
            4,
            SecretSetRef::new("prod-secrets", None).unwrap(),
            now(),
            None,
        )
        .unwrap();
        assert!(can_promote(&release, &wrong_env, &request).is_err());
        // Empty confirmation is rejected.
        let bad_request = PromotionRequest::new(
            release.application_id(),
            env.id(),
            release.id(),
            "owner",
            "",
            None,
        );
        assert!(bad_request.is_err());
    }

    #[test]
    fn rollback_requires_failed_or_deploying_state() {
        let (release, env) = make_release(ReleaseState::Failed);
        let request = RollbackRequest::new(
            release.application_id(),
            env.id(),
            Uuid::new_v4(),
            "owner",
            "ROLLBACK",
        )
        .unwrap();
        assert!(can_rollback(&release, &env, &request).is_ok());
        let (healthy, env) = make_release(ReleaseState::Healthy);
        assert!(can_rollback(&healthy, &env, &request).is_err());
    }

    #[test]
    fn is_mutable_digest_classifies_known_shapes() {
        assert!(is_mutable_digest(""));
        assert!(is_mutable_digest("latest"));
        assert!(is_mutable_digest("main"));
        assert!(is_mutable_digest("sha256:"));
        assert!(!is_mutable_digest("sha256:abcdef"));
        assert!(!is_mutable_digest(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        ));
        // Anything with non-hex char is mutable.
        assert!(is_mutable_digest("sha256:zzz"));
    }

    #[test]
    fn validate_environment_isolation_rejects_cross_kind() {
        let id = Uuid::new_v4();
        let prod_secret = SecretSetRef::new("prod-secrets", None).unwrap();
        let preview_secret = SecretSetRef::new("preview-secrets", None).unwrap();
        let preview = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Preview,
            "preview-7",
            1,
            preview_secret.clone(),
            now(),
            Some(now() + Duration::hours(1)),
        )
        .unwrap();
        // Preview with production secret = rejected.
        assert!(matches!(
            validate_environment_isolation(&preview, &prod_secret),
            Err(ApplicationDeliveryError::PreviewCannotUseProductionSecret)
        ));
        assert!(validate_environment_isolation(&preview, &preview_secret).is_ok());
        let prod = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Production,
            "production",
            1,
            prod_secret.clone(),
            now(),
            None,
        )
        .unwrap();
        assert!(validate_environment_isolation(&prod, &prod_secret).is_ok());
        assert!(validate_environment_isolation(&prod, &preview_secret).is_err());
    }

    #[test]
    fn resolve_environment_returns_kind_specific_set() {
        let id = Uuid::new_v4();
        let prod = SecretSetRef::new("prod-secrets", None).unwrap();
        let preview = SecretSetRef::new("preview-secrets", None).unwrap();
        let p = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Production,
            "production",
            1,
            prod.clone(),
            now(),
            None,
        )
        .unwrap();
        assert_eq!(
            resolve_environment(&p, &prod, &preview).unwrap().name(),
            "prod-secrets"
        );
        let v = Environment::new(
            Uuid::new_v4(),
            id,
            EnvironmentKind::Preview,
            "preview-1",
            1,
            preview.clone(),
            now(),
            Some(now() + Duration::hours(1)),
        )
        .unwrap();
        assert_eq!(
            resolve_environment(&v, &prod, &preview).unwrap().name(),
            "preview-secrets"
        );
    }

    #[test]
    fn redact_diagnostic_strips_secrets_and_pem() {
        let redacted = redact_diagnostic("password=hunter2\n-----BEGIN CERT-----\nMIIB");
        assert!(redacted.contains("<redacted>"));
        assert!(!redacted.contains("hunter2"));
        assert!(redacted.contains("<redacted:pem>"));
    }

    #[test]
    fn build_release_identity_is_deterministic_and_unique() {
        let source = ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/repo.git",
            "abcdef1234567890",
        )
        .unwrap();
        let a = build_release_identity(&source, 1);
        let b = build_release_identity(&source, 1);
        assert_eq!(a, b);
        let c = build_release_identity(&source, 2);
        assert_ne!(a, c);
        let other = ApplicationSource::new(
            ApplicationSourceKind::Git,
            "https://example.com/other.git",
            "abcdef1234567890",
        )
        .unwrap();
        assert_ne!(a, build_release_identity(&other, 1));
    }

    #[cfg(test)]
    mod prop {
        use proptest::prelude::*;

        use super::*;

        fn arb_source() -> impl Strategy<Value = ApplicationSource> {
            (
                "[a-z0-9.-]{3,20}",
                proptest::string::string_regex("[a-f0-9]{7,40}").unwrap(),
            )
                .prop_map(|(url, sha)| {
                    ApplicationSource::new(
                        ApplicationSourceKind::Git,
                        format!("https://example.com/{url}.git"),
                        sha,
                    )
                    .unwrap()
                })
        }

        proptest! {
            #[test]
            fn prop_release_identity_changes_with_epoch_or_source(
                source in arb_source(),
                epoch_a in 0u64..100_000,
                epoch_b in 0u64..100_000,
            ) {
                let id_a = build_release_identity(&source, epoch_a);
                let id_b = build_release_identity(&source, epoch_b);
                if epoch_a == epoch_b {
                    prop_assert_eq!(id_a.clone(), id_b);
                } else {
                    prop_assert_ne!(id_a.clone(), id_b);
                }
                prop_assert!(id_a.starts_with("sha256:"));
            }

            #[test]
            fn prop_is_mutable_digest_conservative(
                kind in proptest::sample::select(vec![
                    ApplicationSourceKind::Git,
                    ApplicationSourceKind::Oci,
                    ApplicationSourceKind::Compose,
                ]),
                prefix in "[a-z0-9:.-]{0,8}",
            ) {
                // Anything that does not look like sha256:hex is rejected.
                let value = format!("{prefix}{kind:?}");
                prop_assert!(is_mutable_digest(&value) || value.starts_with("sha256:"));
            }
        }
    }
}
