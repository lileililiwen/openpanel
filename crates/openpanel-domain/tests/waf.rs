//! Domain invariants for strict per-site WAF policies.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use openpanel_domain::waf::{
    DefaultAction, DryRunRequest, Rule, RuleAction, RuleSet, SimulatedRequest,
};
use proptest::prelude::*;
use uuid::Uuid;

fn path_rule(priority: u16, pattern: &str) -> Rule {
    serde_json::from_value(serde_json::json!({
        "kind": "path_block",
        "id": Uuid::new_v4(),
        "enabled": true,
        "priority": priority,
        "pattern": pattern,
        "method": "GET",
        "action": "deny"
    }))
    .expect("valid path rule fixture")
}

#[test]
fn every_rule_kind_deserializes_with_strict_fields() {
    let fixtures = [
        serde_json::json!({"kind":"rate_limit","id":Uuid::new_v4(),"enabled":true,"priority":1,"zone":"login","rate":10,"burst":20,"nodelay":true}),
        serde_json::json!({"kind":"conn_limit","id":Uuid::new_v4(),"enabled":true,"priority":2,"zone":"clients","per_ip":5}),
        serde_json::json!({"kind":"geo_block","id":Uuid::new_v4(),"enabled":true,"priority":3,"countries":["CN","RU"],"action":"challenge"}),
        serde_json::json!({"kind":"user_agent_block","id":Uuid::new_v4(),"enabled":true,"priority":4,"pattern":"BadBot","action":"deny"}),
        serde_json::json!({"kind":"path_block","id":Uuid::new_v4(),"enabled":true,"priority":5,"pattern":"/wp-admin","method":"POST","action":"deny"}),
        serde_json::json!({"kind":"header_challenge","id":Uuid::new_v4(),"enabled":true,"priority":6,"header":"X-Scanner","value":"yes","challenge":{"kind":"redirect","path":"/challenge"}}),
        serde_json::json!({"kind":"body_size_cap","id":Uuid::new_v4(),"enabled":true,"priority":7,"max_bytes":1048576}),
    ];

    for fixture in fixtures {
        let parsed: Rule = serde_json::from_value(fixture).expect("known rule fixture");
        parsed.validate().expect("known rule validates");
    }
}

#[test]
fn unknown_rule_kind_and_unknown_fields_are_rejected() {
    let unknown_kind = serde_json::json!({
        "kind":"shell_command", "id":Uuid::new_v4(), "enabled":true,
        "priority":1, "command":"rm"
    });
    assert!(serde_json::from_value::<Rule>(unknown_kind).is_err());

    let unknown_field = serde_json::json!({
        "kind":"path_block", "id":Uuid::new_v4(), "enabled":true,
        "priority":1, "pattern":"/admin", "method":null,
        "action":"deny", "command":"ignored"
    });
    assert!(serde_json::from_value::<Rule>(unknown_field).is_err());
}

#[test]
fn ruleset_sorts_by_priority_and_rejects_duplicate_ids() {
    let first = path_rule(20, "/second");
    let duplicate = first.clone();
    let error = RuleSet::new(
        Uuid::new_v4(),
        1,
        DefaultAction::Allow,
        vec![first, duplicate],
    )
    .expect_err("duplicate rule ids must fail");
    assert!(error.to_string().contains("duplicate"));

    let set = RuleSet::new(
        Uuid::new_v4(),
        2,
        DefaultAction::Allow,
        vec![path_rule(20, "/second"), path_rule(10, "/first")],
    )
    .expect("valid ruleset");
    assert_eq!(set.rules()[0].priority(), 10);
    assert_eq!(set.rules()[1].priority(), 20);
}

#[test]
fn dry_run_matches_path_and_method_without_mutating_rule() {
    let rule = path_rule(1, "/wp-admin");
    let request = DryRunRequest {
        rule: rule.clone(),
        request: SimulatedRequest {
            path: "/wp-admin/users".to_owned(),
            method: "GET".to_owned(),
            user_agent: "Browser".to_owned(),
            country: Some("US".to_owned()),
            headers: Default::default(),
            body_bytes: 0,
        },
    };
    let result = request.simulate().expect("simulation succeeds");
    assert!(result.would_match);
    assert_eq!(result.action, RuleAction::Deny);
    assert_eq!(rule.hit_count(), 0);
}

proptest! {
    #[test]
    fn prop_path_patterns_with_nginx_control_bytes_are_rejected(
        prefix in "[a-zA-Z0-9/_-]{0,24}",
        control in prop_oneof![Just(";"), Just("{"), Just("}"), Just("\n"), Just("\r"), Just("\0")],
        suffix in "[a-zA-Z0-9/_-]{0,24}",
    ) {
        let rule = path_rule(1, &format!("/{prefix}{control}{suffix}"));
        prop_assert!(rule.validate().is_err());
    }
}
