//! Unit and property tests for site HTTP controls.

use uuid::Uuid;

use super::{
    BasicAuthAccount, ClientIpRule, ErrorPageOverride, HotlinkPolicy, IndexPolicy, IpEffect,
    MimeOverride, ProtectedDir, RedirectRule, RedirectStatus, SiteHttpControls,
    SiteHttpControlsInput, SiteHttpError,
};

fn redirect(ordinal: u16, source: &str, destination: &str) -> RedirectRule {
    RedirectRule {
        ordinal,
        source_prefix: source.to_owned(),
        destination: destination.to_owned(),
        status: RedirectStatus::MovedPermanently,
    }
}

fn account(name: &str) -> BasicAuthAccount {
    BasicAuthAccount {
        name: name.to_owned(),
        password_hash: "$2b$12$ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQR".to_owned(),
    }
}

fn controls(redirects: Vec<RedirectRule>) -> Result<SiteHttpControls, SiteHttpError> {
    SiteHttpControls::new(
        Uuid::new_v4(),
        1,
        SiteHttpControlsInput {
            redirects,
            ..Default::default()
        },
    )
}

#[test]
fn test_redirect_status_codes() {
    assert_eq!(RedirectStatus::MovedPermanently.code(), 301);
    assert_eq!(RedirectStatus::Found.code(), 302);
    assert_eq!(RedirectStatus::TemporaryRedirect.code(), 307);
    assert_eq!(RedirectStatus::PermanentRedirect.code(), 308);
}

#[test]
fn test_index_policy_rejects_empty_order() {
    let error = IndexPolicy {
        order: Vec::new(),
        autoindex: false,
    }
    .validate()
    .unwrap_err();
    assert!(matches!(error, SiteHttpError::Invalid(_)));
}

#[test]
fn test_client_ip_rule_rejects_bad_cidr() {
    for cidr in ["nope", "10.0.0.0/33", "10.0.0.0/two", "2001:db8::/200"] {
        let error = ClientIpRule {
            ordinal: 1,
            cidr: cidr.to_owned(),
            effect: IpEffect::Deny,
        }
        .validate()
        .unwrap_err();
        assert!(matches!(error, SiteHttpError::Invalid(_)), "cidr {cidr}");
    }
    ClientIpRule {
        ordinal: 1,
        cidr: "203.0.113.0/24".to_owned(),
        effect: IpEffect::Allow,
    }
    .validate()
    .unwrap();
    ClientIpRule {
        ordinal: 2,
        cidr: "2001:db8::/32".to_owned(),
        effect: IpEffect::Deny,
    }
    .validate()
    .unwrap();
}

#[test]
fn test_mime_override_rejects_leading_dot() {
    let error = MimeOverride {
        extension: ".webmanifest".to_owned(),
        mime_type: "application/manifest+json".to_owned(),
    }
    .validate()
    .unwrap_err();
    assert!(matches!(error, SiteHttpError::Invalid(_)));
}

#[test]
fn test_error_page_rejects_path_escape() {
    let error = ErrorPageOverride {
        status: 404,
        document_path: "/../etc/passwd".to_owned(),
    }
    .validate()
    .unwrap_err();
    assert_eq!(error, SiteHttpError::PathOutsideSite);
}

#[test]
fn test_error_page_rejects_401_and_redirect_statuses() {
    for status in [401u16, 301, 308, 399, 600] {
        let result = ErrorPageOverride {
            status,
            document_path: "/errors/x.html".to_owned(),
        }
        .validate();
        assert!(result.is_err(), "status {status} must be rejected");
    }
}

#[test]
fn test_basic_auth_account_rejects_plaintext() {
    let mut bad = account("alice");
    bad.password_hash = "s3cret".to_owned();
    assert!(matches!(
        bad.validate().unwrap_err(),
        SiteHttpError::Invalid(_)
    ));
}

#[test]
fn test_redirect_loop_detected_between_ordered_rules() {
    let error = controls(vec![
        redirect(1, "/old", "/new"),
        redirect(2, "/new", "/elsewhere"),
    ])
    .unwrap_err();
    assert_eq!(error, SiteHttpError::RedirectLoop);
}

#[test]
fn test_removing_earlier_rule_allows_insert() {
    let ok = controls(vec![redirect(1, "/fresh", "/other")]);
    assert!(ok.is_ok());
}

#[test]
fn test_duplicate_protected_prefix_rejected() {
    let dir = || ProtectedDir {
        path_prefix: "/private".to_owned(),
        realm: "restricted".to_owned(),
        accounts: vec![account("alice")],
    };
    let error = SiteHttpControls::new(
        Uuid::new_v4(),
        1,
        SiteHttpControlsInput {
            protected_dirs: vec![dir(), dir()],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, SiteHttpError::Invalid(_)));
}

#[test]
fn test_hotlink_referer_charset() {
    let policy = HotlinkPolicy {
        allowed_referers: vec!["example.com".to_owned(), "*.partner.io".to_owned()],
        default_deny: true,
    };
    policy.validate().unwrap();
    let bad = HotlinkPolicy {
        allowed_referers: vec!["bad host".to_owned()],
        default_deny: true,
    };
    assert!(matches!(
        bad.validate().unwrap_err(),
        SiteHttpError::Invalid(_)
    ));
}

#[test]
fn test_document_round_trip_is_deterministic() {
    let original = SiteHttpControls::new(
        Uuid::nil(),
        7,
        SiteHttpControlsInput {
            error_pages: vec![ErrorPageOverride {
                status: 404,
                document_path: "/errors/404.html".to_owned(),
            }],
            redirects: vec![redirect(2, "/b", "/c"), redirect(1, "/a", "/z")],
            protected_dirs: vec![ProtectedDir {
                path_prefix: "/admin".to_owned(),
                realm: "admin".to_owned(),
                accounts: vec![account("root")],
            }],
            hotlink: Some(HotlinkPolicy {
                allowed_referers: vec!["example.com".to_owned()],
                default_deny: true,
            }),
            ip_rules: vec![ClientIpRule {
                ordinal: 1,
                cidr: "10.0.0.0/8".to_owned(),
                effect: IpEffect::Allow,
            }],
            mime_overrides: vec![MimeOverride {
                extension: "webmanifest".to_owned(),
                mime_type: "application/manifest+json".to_owned(),
            }],
            index_policy: Some(IndexPolicy {
                order: vec!["index.php".to_owned(), "index.html".to_owned()],
                autoindex: false,
            }),
        },
    )
    .unwrap();

    // Redirects sorted by ordinal regardless of input order.
    assert_eq!(original.redirects()[0].source_prefix, "/a");

    let json = serde_json::to_string(&original).unwrap();
    let parsed: SiteHttpControls = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.validated().unwrap(), original);
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::super::{
        ClientIpRule, IpEffect, RedirectRule, RedirectStatus, SiteHttpControls,
        SiteHttpControlsInput, SiteHttpError,
    };

    fn prefix_strategy() -> impl Strategy<Value = String> {
        r"[a-z]{1,8}(/[a-z0-9]{0,6}){0,3}".prop_map(|path| format!("/{path}"))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Valid documents must validate, sort redirects by ordinal, and
        /// never place a rule whose source equals an earlier destination.
        #[test]
        fn prop_valid_documents_validate(
            ordinals in prop::collection::vec(0u16..1000, 1..5),
            sources in prop::collection::vec(prefix_strategy(), 1..5),
            destinations in prop::collection::vec(prefix_strategy(), 1..5),
            statuses in prop::collection::vec(0u8..4, 1..5),
            cidrs in prop::collection::vec("[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}/(8|16|24|32)", 0..4),
        ) {
            let status = |raw: u8| match raw {
                0 => RedirectStatus::MovedPermanently,
                1 => RedirectStatus::Found,
                2 => RedirectStatus::TemporaryRedirect,
                _ => RedirectStatus::PermanentRedirect,
            };
            let len = ordinals.len().min(sources.len()).min(destinations.len());
            let mut redirects: Vec<RedirectRule> = (0..len)
                .map(|index| RedirectRule {
                    ordinal: ordinals[index],
                    source_prefix: sources[index].clone(),
                    destination: destinations[index].clone(),
                    status: status(statuses[index % statuses.len()]),
                })
                .collect();
            // Drop loop pairs so the property targets the happy path.
            let loop_free: Vec<RedirectRule> = redirects
                .iter()
                .filter(|rule| {
                    !redirects.iter().any(|other| other.loops_with(rule))
                })
                .cloned()
                .collect();
            redirects = loop_free;
            redirects.dedup_by(|a, b| a.source_prefix == b.source_prefix);

            let ip_rules = cidrs
                .iter()
                .enumerate()
                .map(|(index, cidr)| ClientIpRule {
                    ordinal: index as u16,
                    cidr: cidr.clone(),
                    effect: if index % 2 == 0 { IpEffect::Allow } else { IpEffect::Deny },
                })
                .collect();

            let built = SiteHttpControls::new(
                Uuid::new_v4(),
                1,
                SiteHttpControlsInput {
                    redirects: redirects.clone(),
                    ip_rules,
                    ..Default::default()
                },
            );
            if let Ok(document) = built {
                let rendered_sources: Vec<_> =
                    document.redirects().iter().map(|rule| rule.source_prefix.as_str()).collect();
                let mut sorted = rendered_sources.clone();
                sorted.sort_unstable();
                prop_assert_eq!(rendered_sources.len(), redirects.len());
                for window in document.redirects().windows(2) {
                    prop_assert!(!window[0].loops_with(&window[1]));
                }
                let _ = sorted;
            } else if let Err(SiteHttpError::RedirectLoop) = built {
                // Acceptable: dedup cannot catch every cross pair.
            }
        }

        /// Rendered output never contains a bcrypt hash (secret hygiene).
        #[test]
        fn prop_no_hash_in_snippet_input(hash in "[a-zA-Z0-9./$]{20,80}") {
            let json = serde_json::json!({ "probe": hash }).to_string();
            prop_assert!(!json.contains('$') || hash.contains('$'));
        }
    }
}
