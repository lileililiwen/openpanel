#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use openpanel_domain::mail::{
    AliasGraph, DeletionToken, MailAddress, MailDomainName, MailQuota, OutboundRateLimit,
    RelayPolicy,
};
use proptest::prelude::*;

#[test]
fn domain_address_quota_and_deletion_token_validate() {
    let domain = MailDomainName::new("Example.COM.").unwrap();
    assert_eq!(domain.as_str(), "example.com");
    assert!(MailDomainName::new("éxample.com").is_err());
    assert_eq!(
        MailAddress::new("Alice", domain.clone()).unwrap().as_str(),
        "alice@example.com"
    );
    assert!(MailAddress::parse("bad local@example.com").is_err());
    assert!(MailQuota::new(0, 1024, 1_048_576).is_err());
    assert_eq!(MailQuota::new(4096, 1024, 1_048_576).unwrap().bytes(), 4096);
    let rate = OutboundRateLimit::new(100, 3600).unwrap();
    assert!(rate.permits(99));
    assert!(!rate.permits(100));
    assert_eq!(rate.window_seconds(), 3600);
    assert!(OutboundRateLimit::new(0, 3600).is_err());
    let token = DeletionToken::issue("example.com", 100, 30).unwrap();
    assert!(token.verify("example.com", 120));
    assert!(!token.verify("example.com", 131));
}

#[test]
fn alias_graph_rejects_direct_and_transitive_cycles() {
    let mut graph = AliasGraph::default();
    graph
        .add(
            MailAddress::parse("a@example.com").unwrap(),
            MailAddress::parse("b@example.com").unwrap(),
        )
        .unwrap();
    graph
        .add(
            MailAddress::parse("b@example.com").unwrap(),
            MailAddress::parse("c@example.com").unwrap(),
        )
        .unwrap();
    assert!(
        graph
            .add(
                MailAddress::parse("c@example.com").unwrap(),
                MailAddress::parse("a@example.com").unwrap()
            )
            .is_err()
    );
}

#[test]
fn unauthenticated_third_party_relay_is_always_denied() {
    let policy = RelayPolicy::new(vec![MailDomainName::new("local.example").unwrap()]);
    assert!(!policy.allows(
        false,
        &MailAddress::parse("sender@outside.test").unwrap(),
        &MailAddress::parse("target@remote.test").unwrap()
    ));
    assert!(policy.allows(
        true,
        &MailAddress::parse("alice@local.example").unwrap(),
        &MailAddress::parse("target@remote.test").unwrap()
    ));
}

proptest! {
    #[test]
    fn prop_address_canonicalization_is_stable(local in "[a-zA-Z0-9._-]{1,32}") {
        let raw=format!("{}@EXAMPLE.COM.",local);
        if let Ok(address)=MailAddress::parse(&raw){prop_assert_eq!(MailAddress::parse(address.as_str()).unwrap(),address);}
    }
}
