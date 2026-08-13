//! Application contracts for WAF compilation, atomic replacement, and hits.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use openpanel_app::waf::{NginxSnippetCompiler, WafConfigApplier, WafService};
use openpanel_core::{AuditAction, AuditOutcome};
use openpanel_domain::{
    Email, Password, RepoError, Role, Site, User, Username,
    waf::{DefaultAction, Rule, RuleSet, WafError, WafHit, WafRepository},
};
use openpanel_test_support::{MockAudit, MockSiteRepo};
use uuid::Uuid;

mockall::mock! {
    Repo {}
    #[async_trait]
    impl WafRepository for Repo {
        async fn get(&self, site_id: Uuid) -> Result<Option<RuleSet>, RepoError>;
        async fn put(&self, ruleset: &RuleSet) -> Result<(), RepoError>;
        async fn record_hit(&self, hit: &WafHit) -> Result<(), RepoError>;
        async fn hits(&self, site_id: Uuid) -> Result<Vec<WafHit>, RepoError>;
    }
}

mockall::mock! {
    Applier {}
    #[async_trait]
    impl WafConfigApplier for Applier {
        async fn apply(&self, site: &Site, snippet: &str) -> Result<(), WafError>;
    }
}

fn site() -> Site {
    Site::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "waf.example.test",
        vec![],
        "/var/www/waf.example.test/public_html",
        false,
        None,
        "owner",
    )
    .expect("site fixture")
}

fn user(role: Role) -> User {
    User::new(
        Uuid::new_v4(),
        Username::new(if role == Role::Owner {
            "owner"
        } else {
            "admin"
        })
        .expect("username"),
        Email::new(if role == Role::Owner {
            "owner@example.test"
        } else {
            "admin@example.test"
        })
        .expect("email"),
        Password::hash("correct horse battery staple").expect("password"),
        role,
    )
}

fn rules(site_id: Uuid) -> RuleSet {
    let path: Rule = serde_json::from_value(serde_json::json!({
        "kind":"path_block", "id":Uuid::new_v4(), "enabled":true,
        "priority":20, "pattern":"/wp-admin", "method":"POST", "action":"deny"
    }))
    .expect("path rule");
    let body: Rule = serde_json::from_value(serde_json::json!({
        "kind":"body_size_cap", "id":Uuid::new_v4(), "enabled":true,
        "priority":10, "max_bytes":1048576
    }))
    .expect("body rule");
    RuleSet::new(site_id, 1, DefaultAction::Allow, vec![path, body]).expect("ruleset")
}

#[test]
fn compiler_is_stable_orders_rules_and_empty_is_empty() {
    let site_id = Uuid::new_v4();
    let rules = rules(site_id);
    let first = NginxSnippetCompiler::compile(&rules).expect("compile");
    let second = NginxSnippetCompiler::compile(&rules).expect("compile twice");
    assert_eq!(first, second);
    assert!(first.starts_with(&format!("# openpanel-waf site={site_id} rev=1")));
    assert!(
        first.find("client_max_body_size").expect("body directive")
            < first.find("wp-admin").expect("path directive")
    );
    assert_eq!(
        NginxSnippetCompiler::compile(&RuleSet::empty(site_id)).expect("empty compile"),
        ""
    );
}

#[test]
fn every_rule_kind_compiles_without_client_control_tokens() {
    let values = [
        serde_json::json!({"kind":"rate_limit","id":Uuid::new_v4(),"enabled":true,"priority":1,"zone":"login","rate":10,"burst":20,"nodelay":true}),
        serde_json::json!({"kind":"conn_limit","id":Uuid::new_v4(),"enabled":true,"priority":2,"zone":"clients","per_ip":5}),
        serde_json::json!({"kind":"geo_block","id":Uuid::new_v4(),"enabled":true,"priority":3,"countries":["CN"],"action":"deny"}),
        serde_json::json!({"kind":"user_agent_block","id":Uuid::new_v4(),"enabled":true,"priority":4,"pattern":"BadBot","action":"deny"}),
        serde_json::json!({"kind":"path_block","id":Uuid::new_v4(),"enabled":true,"priority":5,"pattern":"/private","method":null,"action":"deny"}),
        serde_json::json!({"kind":"header_challenge","id":Uuid::new_v4(),"enabled":true,"priority":6,"header":"X-Test","value":"yes","challenge":{"kind":"tarpit"}}),
        serde_json::json!({"kind":"body_size_cap","id":Uuid::new_v4(),"enabled":true,"priority":7,"max_bytes":2048}),
    ];
    for value in values {
        let rule: Rule = serde_json::from_value(value).expect("rule fixture");
        let set =
            RuleSet::new(Uuid::new_v4(), 1, DefaultAction::Allow, vec![rule]).expect("ruleset");
        let snippet = NginxSnippetCompiler::compile(&set).expect("compile");
        assert!(!snippet.contains("\0"));
    }
}

#[tokio::test]
async fn put_applies_before_persisting_and_is_idempotent() {
    let site = site();
    let site_id = site.id();
    let desired = rules(site_id);
    let rules_for_get = desired.clone();
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .times(2)
        .returning(move |_| Ok(Some(site.clone())));
    let mut repo = MockRepo::new();
    repo.expect_get()
        .times(2)
        .returning(move |_| Ok(Some(rules_for_get.clone())));
    repo.expect_put().never();
    let mut applier = MockApplier::new();
    applier.expect_apply().never();
    let service = WafService::new(
        Arc::new(repo),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
        Arc::new(applier),
    );

    let result = service
        .put(&user(Role::Owner), desired.clone())
        .await
        .expect("idempotent put");
    assert_eq!(result, desired);
    let result = service
        .put(&user(Role::Owner), desired.clone())
        .await
        .expect("second idempotent put");
    assert_eq!(result, desired);
}

#[tokio::test]
async fn compile_failure_does_not_persist_ruleset() {
    let site = site();
    let site_id = site.id();
    let desired = rules(site_id);
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .once()
        .returning(move |_| Ok(Some(site.clone())));
    let mut repo = MockRepo::new();
    repo.expect_get().once().returning(|_| Ok(None));
    repo.expect_put().never();
    let mut applier = MockApplier::new();
    applier
        .expect_apply()
        .once()
        .returning(|_, _| Err(WafError::Compile("nginx -t rejected candidate".into())));
    let service = WafService::new(
        Arc::new(repo),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
        Arc::new(applier),
    );

    assert!(matches!(
        service.put(&user(Role::Owner), desired).await,
        Err(WafError::Compile(_))
    ));
}

#[tokio::test]
async fn non_owner_is_rejected_before_ports_are_touched() {
    let mut sites = MockSiteRepo::new();
    sites.expect_find_by_id().never();
    let mut repo = MockRepo::new();
    repo.expect_get().never();
    repo.expect_put().never();
    let mut applier = MockApplier::new();
    applier.expect_apply().never();
    let service = WafService::new(
        Arc::new(repo),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
        Arc::new(applier),
    );
    assert_eq!(
        service.get(&user(Role::Admin), Uuid::new_v4()).await,
        Err(WafError::Forbidden)
    );
}

#[tokio::test]
async fn sampler_records_bounded_hit_metadata() {
    let site_id = Uuid::new_v4();
    let rule_id = Uuid::new_v4();
    let now = Utc::now();
    let mut repo = MockRepo::new();
    repo.expect_record_hit()
        .once()
        .withf(move |hit| {
            hit.site_id == site_id
                && hit.rule_id == rule_id
                && hit.kind == "path_block"
                && hit.count == 1
        })
        .returning(|_| Ok(()));
    let service = WafService::new(
        Arc::new(repo),
        Arc::new(MockSiteRepo::new()),
        Arc::new(MockAudit::stub()),
        Arc::new(MockApplier::new()),
    );
    service
        .sample_hit(WafHit {
            site_id,
            rule_id,
            kind: "path_block".to_owned(),
            action: openpanel_domain::waf::RuleAction::Deny,
            count: 1,
            last_triggered_at: now,
        })
        .await
        .expect("hit recorded");
}

#[tokio::test]
async fn sampler_spike_emits_monitoring_alert_event() {
    let site_id = Uuid::new_v4();
    let rule_id = Uuid::new_v4();
    let mut repo = MockRepo::new();
    repo.expect_record_hit().once().returning(|_| Ok(()));
    let mut audit = MockAudit::new();
    audit
        .expect_record()
        .once()
        .withf(move |event| {
            event.action == AuditAction::AlertFired
                && event.outcome == AuditOutcome::Success
                && event.target.as_deref() == Some(site_id.to_string().as_str())
                && event.metadata["metric"] == "waf.hits"
        })
        .returning(|_| Ok(()));
    audit.expect_recent().returning(|_| Ok(vec![]));
    let service = WafService::new(
        Arc::new(repo),
        Arc::new(MockSiteRepo::new()),
        Arc::new(audit),
        Arc::new(MockApplier::new()),
    )
    .with_spike_threshold(2);
    service
        .sample_hit(WafHit {
            site_id,
            rule_id,
            kind: "path_block".to_owned(),
            action: openpanel_domain::waf::RuleAction::Deny,
            count: 2,
            last_triggered_at: Utc::now(),
        })
        .await
        .expect("spike recorded");
}
