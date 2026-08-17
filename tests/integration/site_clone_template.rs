//! Site clone + template export integration tests: plan/run
//! lifecycle, template export, and signature verification.
//!
//! Live-clone plan calls go through `SitesService::get_site` to
//! compute the source content hash. These tests exercise the
//! *typed errors* of the domain `ClonePlan::new` constructor
//! directly (which is where domain validation lives) and the
//! service-level error translation for non-existent sites.

use openpanel_domain::{ClonePlan, CloneSource, DbAction, PiiPolicy, SiteCloneTemplateError};
use uuid::Uuid;

use crate::common::*;

#[test]
fn plan_constructor_rejects_empty_target_domain() {
    let err = ClonePlan::new(
        Uuid::new_v4(),
        CloneSource::Site {
            site_id: Uuid::new_v4(),
        },
        Uuid::new_v4(),
        "",
        PiiPolicy::Standard,
        vec![],
        DbAction::None,
        vec![],
        "abc",
        chrono::Utc::now(),
    )
    .expect_err("empty target domain must be rejected");
    assert!(matches!(
        err,
        SiteCloneTemplateError::InvalidTargetDomain(_)
    ));
}

#[test]
fn plan_constructor_rejects_oversized_target_domain() {
    let long = "a".repeat(254);
    let err = ClonePlan::new(
        Uuid::new_v4(),
        CloneSource::Site {
            site_id: Uuid::new_v4(),
        },
        Uuid::new_v4(),
        long,
        PiiPolicy::Standard,
        vec![],
        DbAction::None,
        vec![],
        "abc",
        chrono::Utc::now(),
    )
    .expect_err("oversized target domain must be rejected");
    assert!(matches!(
        err,
        SiteCloneTemplateError::InvalidTargetDomain(_)
    ));
}

#[test]
fn plan_constructor_accepts_valid_input() {
    let plan = ClonePlan::new(
        Uuid::new_v4(),
        CloneSource::Site {
            site_id: Uuid::new_v4(),
        },
        Uuid::new_v4(),
        "staging.example.com",
        PiiPolicy::Standard,
        vec![],
        DbAction::DumpAndLoad { est_size_bytes: 0 },
        vec!["warn".to_string()],
        "abc",
        chrono::Utc::now(),
    )
    .expect("plan");
    assert_eq!(plan.target_domain(), "staging.example.com");
    assert!(matches!(plan.source(), CloneSource::Site { .. }));
    assert!(matches!(plan.pii_policy(), PiiPolicy::Standard));
    assert!(!plan.content_hash().is_empty());
}

#[tokio::test]
async fn plan_live_clone_for_missing_source_returns_refused() {
    let server = TestServer::new().await;
    let svc = server.site_clone_template();

    // The random site id will not exist in the test server; the
    // service refuses the plan with `Refused("source: …")`. This
    // is the correct behaviour — content-hash computation requires
    // a real source site.
    let err = svc
        .plan_live_clone(
            "admin",
            Uuid::new_v4(),
            "staging.example.com",
            Uuid::new_v4(),
            PiiPolicy::Standard,
        )
        .await
        .expect_err("non-existent source site must be refused");
    assert!(matches!(err, SiteCloneTemplateError::Refused(_)));
}

#[test]
fn pii_policy_wire_names() {
    use openpanel_domain::PiiPolicy as P;
    assert_eq!(P::Standard.as_str(), "standard");
    assert_eq!(P::Keep.as_str(), "keep");
    assert_eq!(P::None.as_str(), "none");
}
