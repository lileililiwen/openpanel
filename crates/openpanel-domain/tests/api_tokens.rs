//! API-token domain invariants and property tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use chrono::{Duration, Utc};
use openpanel_domain::api_tokens::{ApiToken, Cidr, TokenCredential, TokenScope};
use proptest::prelude::*;
use uuid::Uuid;

#[test]
fn token_credential_round_trips_and_rejects_changed_checksum() {
    let credential = TokenCredential::generate();
    let rendered = credential.expose();
    assert!(rendered.starts_with("openpanel_pat_"));
    assert_eq!(rendered.len(), 43);
    assert_eq!(TokenCredential::parse(rendered).unwrap(), credential);

    let mut invalid = rendered.to_string();
    let last = invalid.pop().unwrap();
    invalid.push(if last == 'A' { 'B' } else { 'A' });
    assert!(TokenCredential::parse(&invalid).is_err());
}

#[test]
fn token_hash_is_deterministic_and_bound_to_pepper() {
    let token = TokenCredential::parse("openpanel_pat_AAAAAAAAAAAAAAAAAAAAAAAA.AAAA");
    assert!(token.is_err(), "fixture checksum must be validated");

    let token = TokenCredential::generate();
    assert_eq!(token.hash(&[7; 32]), token.hash(&[7; 32]));
    assert_ne!(token.hash(&[7; 32]), token.hash(&[8; 32]));
    assert!(!token.hash(&[7; 32]).as_hex().contains(token.body()));
}

#[test]
fn scope_parser_and_set_operations_are_exact() {
    let read = TokenScope::parse("sites:read").unwrap();
    let write = TokenScope::parse("sites:write").unwrap();
    assert_eq!(read.context(), "sites");
    assert_eq!(read.verb(), "read");
    assert!(TokenScope::parse("sites").is_err());
    assert!(TokenScope::parse("Sites:read").is_err());
    assert!(TokenScope::parse("sites:read:all").is_err());

    let scopes = BTreeSet::from([read.clone()]);
    assert!(scopes.contains(&read));
    assert!(!scopes.contains(&write));
}

#[test]
fn cidr_matches_ipv4_and_ipv6_prefixes() {
    let local = Cidr::parse("192.168.8.0/24").unwrap();
    assert!(local.contains("192.168.8.42".parse().unwrap()));
    assert!(!local.contains("192.168.9.1".parse().unwrap()));
    let v6 = Cidr::parse("2001:db8::/32").unwrap();
    assert!(v6.contains("2001:db8::1234".parse().unwrap()));
    assert!(!v6.contains("2001:db9::1".parse().unwrap()));
    assert!(Cidr::parse("192.168.1.1/33").is_err());
}

#[test]
fn aggregate_tracks_expiry_revocation_and_never_exposes_hash_in_metadata() {
    let now = Utc::now();
    let credential = TokenCredential::generate();
    let mut token = ApiToken::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "deploy bot".into(),
        credential.hash(&[9; 32]),
        BTreeSet::from([TokenScope::parse("sites:read").unwrap()]),
        vec![],
        now + Duration::days(1),
        now,
    )
    .unwrap();
    assert!(token.is_active(now));
    assert!(!token.is_active(now + Duration::days(2)));
    token.revoke(now + Duration::hours(1)).unwrap();
    assert!(!token.is_active(now + Duration::hours(1)));
    let json = serde_json::to_string(&token.metadata()).unwrap();
    assert!(!json.contains(&token.hash().as_hex()));
    assert!(!json.contains(credential.body()));
}

proptest! {
    #[test]
    fn prop_random_credentials_round_trip_and_hashes_change_with_body(_seed in any::<[u8; 32]>()) {
        let first = TokenCredential::generate();
        let second = TokenCredential::generate();
        prop_assert_eq!(TokenCredential::parse(first.expose()).unwrap(), first.clone());
        prop_assert_ne!(first.hash(&_seed), second.hash(&_seed));
    }

    #[test]
    fn prop_scope_round_trip(context in "[a-z][a-z0-9_-]{0,15}", verb in "[a-z][a-z0-9_-]{0,15}") {
        let text = format!("{context}:{verb}");
        let scope = TokenScope::parse(&text).unwrap();
        prop_assert_eq!(scope.to_string(), text);
    }
}
