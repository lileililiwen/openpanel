//! FTP domain invariant tests.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use chrono::Utc;
use openpanel_domain::ftp::{FtpAccount, FtpLimits, FtpPath};
use proptest::prelude::*;
use uuid::Uuid;

fn account(username: &str, password: &str) -> Result<FtpAccount, openpanel_domain::ftp::FtpError> {
    FtpAccount::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        username,
        PathBuf::from("/srv/openpanel/sites/example.test/public"),
        password,
        false,
        FtpLimits::default(),
        Utc::now(),
    )
}

#[test]
fn ftp_account_validates_username_password_and_absolute_home() {
    assert!(account("uploads", "correct horse battery staple").is_ok());
    assert!(account("../escape", "correct horse battery staple").is_err());
    assert!(account("UPPER", "correct horse battery staple").is_err());
    assert!(account("uploads", "short").is_err());

    let relative = FtpAccount::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "uploads",
        PathBuf::from("relative/root"),
        "correct horse battery staple",
        false,
        FtpLimits::default(),
        Utc::now(),
    );
    assert!(relative.is_err());
}

#[test]
fn ftp_account_enabled_transitions_and_password_verification_work() {
    let mut value = account("uploads", "correct horse battery staple").unwrap();
    assert!(value.enabled());
    assert!(
        value
            .verify_password("correct horse battery staple")
            .unwrap()
    );
    assert!(!value.verify_password("wrong password value").unwrap());
    value.disable(Utc::now());
    assert!(!value.enabled());
    assert!(value.disabled_at().is_some());
    value.enable();
    assert!(value.enabled());
    assert!(value.disabled_at().is_none());
}

#[test]
fn ftp_account_enforces_session_limit() {
    let value = account("uploads", "correct horse battery staple").unwrap();
    assert!(value.can_open_session(3));
    assert!(!value.can_open_session(4));
}

proptest! {
    #[test]
    fn prop_parent_segments_are_always_rejected(prefix in "[A-Za-z0-9_-]{0,24}", suffix in "[A-Za-z0-9_.-]{0,24}") {
        let raw = format!("{prefix}/../{suffix}");
        prop_assert!(FtpPath::new(&raw).is_err());
    }

    #[test]
    fn prop_absolute_paths_are_always_rejected(parts in prop::collection::vec("[A-Za-z0-9_-]{1,16}", 1..8)) {
        let raw = format!("/{}", parts.join("/"));
        prop_assert!(FtpPath::new(&raw).is_err());
    }

    #[test]
    fn prop_session_limit_is_total(active in 0_u16..1024, max in 1_u16..128) {
        let limits = FtpLimits::new(1_048_576, max).unwrap();
        prop_assert_eq!(limits.allows_session(active), active < max);
    }
}
