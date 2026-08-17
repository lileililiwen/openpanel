//! Themeable UI / white-label integration tests: contrast
//! enforcement, override CRUD, and host-header resolution.

use openpanel_domain::{
    BrandingScope, HexColor, Palette, PanelDomain, ThemeableUiError, Typography,
};
use uuid::Uuid;

use crate::common::*;

#[tokio::test]
async fn palette_rejects_low_contrast() {
    let fg = HexColor::parse("#aaa").unwrap();
    let bg = HexColor::parse("#bbb").unwrap();
    let accent = HexColor::parse("#000").unwrap();
    let err = Palette::with_min_contrast(fg, bg, accent, 4.5).expect_err("reject");
    match err {
        ThemeableUiError::InsufficientContrast { pair, ratio } => {
            assert_eq!(pair, "fg:bg");
            assert!(ratio < 4.5);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test]
async fn palette_accepts_strong_contrast() {
    let fg = HexColor::parse("#fff").unwrap();
    let bg = HexColor::parse("#000").unwrap();
    let accent = HexColor::parse("#0066cc").unwrap();
    Palette::with_min_contrast(fg, bg, accent, 4.5).expect("accept");
}

#[tokio::test]
async fn override_lifecycle_via_service() {
    let server = TestServer::new().await;
    let svc = server.themeable_ui();

    let owner_id = Uuid::new_v4();
    let palette = Palette::with_min_contrast(
        HexColor::parse("#fff").unwrap(),
        HexColor::parse("#000").unwrap(),
        HexColor::parse("#0066cc").unwrap(),
        4.5,
    )
    .unwrap();
    let typo = Typography::new("system-ui", 16).unwrap();
    let panel = PanelDomain::new("panel.acme.com").unwrap();
    let next = svc
        .set_override(
            "admin",
            owner_id,
            "Acme",
            palette.clone(),
            typo.clone(),
            Some(panel.clone()),
            BrandingScope::Reseller,
        )
        .await
        .expect("set");
    assert_eq!(next.brand_name(), "Acme");
    assert_eq!(
        next.panel_domain().map(|d| d.fqdn().to_string()),
        Some("panel.acme.com".to_string())
    );

    let loaded = svc.get_override("admin", owner_id).await.expect("get");
    let loaded = loaded.expect("present");
    assert_eq!(loaded.brand_name(), "Acme");

    // resolve_for_host returns the override for the panel FQDN.
    let resolved = svc
        .resolve_for_host("panel.acme.com")
        .await
        .expect("resolve")
        .expect("found");
    assert_eq!(resolved.brand_name(), "Acme");

    // clear
    svc.clear_override("admin", owner_id, BrandingScope::Reseller)
        .await
        .expect("clear");
    let after = svc.get_override("admin", owner_id).await.expect("get2");
    assert!(after.is_none());
}

#[tokio::test]
async fn override_refused_for_disabled_branding() {
    let server = TestServer::new().await;
    let svc = server.themeable_ui();

    let owner_id = Uuid::new_v4();
    let palette = Palette::with_min_contrast(
        HexColor::parse("#fff").unwrap(),
        HexColor::parse("#000").unwrap(),
        HexColor::parse("#0066cc").unwrap(),
        4.5,
    )
    .unwrap();
    let typo = Typography::new("system-ui", 16).unwrap();
    let err = svc
        .set_override(
            "admin",
            owner_id,
            "Acme",
            palette,
            typo,
            None,
            BrandingScope::Disabled,
        )
        .await
        .expect_err("disabled branding must refuse");
    assert!(matches!(err, ThemeableUiError::BrandingNotAllowed));
}

#[tokio::test]
async fn panel_domain_rejects_invalid_fqdn() {
    let err = PanelDomain::new("-bad.example.com").expect_err("reject leading dash");
    assert!(matches!(err, ThemeableUiError::InvalidPanelDomain(_)));
}

#[tokio::test]
async fn branding_scope_wire_names() {
    assert_eq!(BrandingScope::Reseller.as_str(), "reseller");
    assert_eq!(BrandingScope::Disabled.as_str(), "disabled");
    assert_eq!(BrandingScope::System.as_str(), "system");
}
