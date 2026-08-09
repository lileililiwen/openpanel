#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use openpanel_domain::dns::{
    DnsName, DnsRecord, RecordData, RecordKind, RemoteVersion, Ttl, validate_record_set,
};
use proptest::prelude::*;
use uuid::Uuid;

#[test]
fn dns_names_and_ttls_are_canonical_and_bounded() {
    assert_eq!(
        DnsName::new("WWW.Example.COM.").unwrap().as_str(),
        "www.example.com"
    );
    assert!(DnsName::new("bad name.example").is_err());
    assert!(DnsName::new("*.example.com").is_err());
    assert!(Ttl::new(30, 60, 86_400).is_err());
    assert_eq!(Ttl::new(300, 60, 86_400).unwrap().get(), 300);
}

#[test]
fn every_supported_record_type_validates_and_normalizes() {
    let cases = [
        (
            RecordKind::A,
            RecordData::parse(RecordKind::A, "192.0.2.1").unwrap(),
        ),
        (
            RecordKind::Aaaa,
            RecordData::parse(RecordKind::Aaaa, "2001:db8::1").unwrap(),
        ),
        (
            RecordKind::Cname,
            RecordData::parse(RecordKind::Cname, "Target.Example.").unwrap(),
        ),
        (
            RecordKind::Txt,
            RecordData::parse(RecordKind::Txt, "verification=value").unwrap(),
        ),
        (
            RecordKind::Mx,
            RecordData::parse(RecordKind::Mx, "10 Mail.Example.").unwrap(),
        ),
        (
            RecordKind::Caa,
            RecordData::parse(RecordKind::Caa, "0 issue letsencrypt.org").unwrap(),
        ),
        (
            RecordKind::Ns,
            RecordData::parse(RecordKind::Ns, "NS1.Example.").unwrap(),
        ),
        (
            RecordKind::Srv,
            RecordData::parse(RecordKind::Srv, "10 5 443 Service.Example.").unwrap(),
        ),
    ];
    for (kind, data) in cases {
        assert_eq!(data.kind(), kind);
        assert_eq!(RecordData::parse(kind, &data.to_string()).unwrap(), data);
    }
}

#[test]
fn cname_exclusivity_is_checked_before_provider_use() {
    let name = DnsName::new("www.example.com").unwrap();
    let existing = DnsRecord::new(
        Uuid::new_v4(),
        name.clone(),
        RecordData::parse(RecordKind::A, "192.0.2.1").unwrap(),
        Ttl::new(300, 60, 86_400).unwrap(),
        "remote-a",
        RemoteVersion::new("v1").unwrap(),
    );
    let cname = DnsRecord::new(
        Uuid::new_v4(),
        name,
        RecordData::parse(RecordKind::Cname, "origin.example.com").unwrap(),
        Ttl::new(300, 60, 86_400).unwrap(),
        "remote-c",
        RemoteVersion::new("v1").unwrap(),
    );
    assert!(validate_record_set(&[existing, cname]).is_err());
}

proptest! {
    #[test]
    fn prop_txt_parse_render_round_trips(value in "[A-Za-z0-9_=][A-Za-z0-9_= -]{0,179}") {
        let parsed = RecordData::parse(RecordKind::Txt, &value).unwrap();
        prop_assert_eq!(RecordData::parse(RecordKind::Txt, &parsed.to_string()).unwrap(), parsed);
    }

    #[test]
    fn prop_invalid_dns_label_characters_never_survive(label in "[^a-zA-Z0-9.-]{1,20}") {
        let candidate = format!("{}.example.com", label);
        prop_assert!(DnsName::new(&candidate).is_err());
    }
}
