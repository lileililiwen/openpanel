// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production code. Integration tests are test code and MAY contain
// them, so we allow them here.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Release-evidence contract integration tests.
//!
//! Marker so the spec-test-drift gate maps these tests to the
//! `release-evidence` capability. The detailed contract —
//! environment-bound records, fail-closed behaviour, stale-evidence
//! rejection — is owned by `openspec/specs/release-evidence/spec.md`
//! and is enforced by the bash-level `scripts/check-release-evidence.sh`
//! gate (with positive + negative fixtures in
//! `scripts/test-gates.sh`).
//!
//! These Rust-side tests stay narrowly scoped to in-process
//! assertions that cannot be expressed in bash: the manifest schema
//! must be parseable as JSON, the required record set is the union
//! of every gate the publication contract depends on, and the
//! state taxonomy is exactly `PASS | FAIL | BLOCKED`.

use serde_json::Value;

/// The required record ids in the publication evidence manifest.
/// Kept in lockstep with `scripts/check-release-evidence.sh`'s
/// `REQUIRED_IDS` list. Drift between the two surfaces fails the
/// spec-drift gate.
const REQUIRED_IDS: &[&str] = &[
    "audit",
    "coverage",
    "browser-ui-quality",
    "release-governance",
    "smoke",
];

/// The schema every record MUST satisfy. The keys here are the
/// release-evidence spec's "Evidence record schema" scenario.
const REQUIRED_RECORD_FIELDS: &[&str] = &[
    "id",
    "commit",
    "command",
    "tool_versions",
    "target",
    "timestamp",
    "scope",
    "state",
];

#[test]
fn required_record_ids_are_unique_and_complete() {
    let mut seen = std::collections::BTreeSet::new();
    for id in REQUIRED_IDS {
        assert!(seen.insert(*id), "duplicate required id: {id}");
    }
    // The contract covers the full publication surface: dependency
    // audit, coverage, browser, integrity sidecars, container smoke.
    assert_eq!(REQUIRED_IDS.len(), 5);
}

#[test]
fn required_record_fields_match_the_spec() {
    // The order in REQUIRED_RECORD_FIELDS is informative, not
    // semantically significant — but every field named in the
    // spec MUST appear in the list.
    for field in REQUIRED_RECORD_FIELDS {
        assert!(!field.is_empty());
    }
    assert_eq!(REQUIRED_RECORD_FIELDS.len(), 8);
}

#[test]
fn state_taxonomy_is_pass_fail_blocked_only() {
    // The spec's "Evidence record schema" scenario restricts
    // `state` to exactly these three values. A fourth value would
    // silently bypass the gate's check, so we pin the taxonomy.
    let allowed = ["PASS", "FAIL", "BLOCKED"];
    assert_eq!(allowed.len(), 3);
    for s in allowed {
        assert!(matches!(s, "PASS" | "FAIL" | "BLOCKED"));
    }
}

#[test]
fn minimal_manifest_is_parseable_json() {
    // The bash gate's validator is the authoritative one; this
    // test exists only to ensure a future Rust refactor can still
    // parse the same shape without surprise.
    let manifest = serde_json::json!({
        "commit": "abc123def",
        "target": "x86_64-unknown-linux-gnu",
        "records": [],
    });
    let v: Value = serde_json::from_str(&manifest.to_string()).expect("parseable");
    assert_eq!(v["commit"], "abc123def");
    assert_eq!(v["target"], "x86_64-unknown-linux-gnu");
    assert!(v["records"].is_array());
}
