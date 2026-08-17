//! Themeable UI and white-label bounded context: per-account
//! `ThemeOverride`, `Palette` with WCAG-AA contrast enforcement,
//! `Typography`, and `PanelDomain` FQDN routing. The change also
//! adds the `BrandingScope` (Reseller | Disabled | System) enum on
//! the existing `HostingPlan` so the API can gate branding by
//! plan.
//!
//! I/O-free: the actual `tokens.css` rendering, SVG sanitisation,
//! and the file-system logo upload live in the application layer.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// A pair of CSS hex colours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HexColor(String);

impl HexColor {
    /// Parse a `#rrggbb` or `#rgb` colour. Lower-case hex is
    /// normalised.
    pub fn parse(input: impl AsRef<str>) -> Result<Self, ThemeableUiError> {
        let s = input.as_ref().trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.len() != 3 && s.len() != 6 {
            return Err(ThemeableUiError::InvalidColor(s.to_string()));
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ThemeableUiError::InvalidColor(s.to_string()));
        }
        let normalised = if s.len() == 3 {
            let mut out = String::with_capacity(6);
            for c in s.chars() {
                out.push(c);
                out.push(c);
            }
            out
        } else {
            s.to_lowercase()
        };
        Ok(Self(format!("#{normalised}")))
    }

    /// The colour as `#rrggbb`.
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Linearised sRGB channel value (0..=1).
    fn linear_channel(&self) -> [f64; 3] {
        let h = self.0.trim_start_matches('#');
        let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0) as f64 / 255.0;
        let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0) as f64 / 255.0;
        let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0) as f64 / 255.0;
        [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)]
    }
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(rgb: [f64; 3]) -> f64 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// WCAG-AA contrast ratio between two colours.
pub fn contrast_ratio(fg: &HexColor, bg: &HexColor) -> f64 {
    let l1 = relative_luminance(fg.linear_channel());
    let l2 = relative_luminance(bg.linear_channel());
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// A typed palette: foreground, background, accent, and a
/// `contrast_min` floor. The constructor enforces WCAG-AA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    color_fg: HexColor,
    color_bg: HexColor,
    color_accent: HexColor,
    contrast_min: f64,
}

impl Palette {
    /// Construct a palette with default `contrast_min = 4.5`.
    pub fn new(fg: HexColor, bg: HexColor, accent: HexColor) -> Result<Self, ThemeableUiError> {
        Self::with_min_contrast(fg, bg, accent, 4.5)
    }

    /// Construct a palette with an explicit minimum contrast.
    pub fn with_min_contrast(
        fg: HexColor,
        bg: HexColor,
        accent: HexColor,
        contrast_min: f64,
    ) -> Result<Self, ThemeableUiError> {
        if contrast_min < 1.0 || contrast_min > 21.0 {
            return Err(ThemeableUiError::InvalidContrastMin(contrast_min));
        }
        let ratio = contrast_ratio(&fg, &bg);
        if ratio + f64::EPSILON < contrast_min {
            return Err(ThemeableUiError::InsufficientContrast {
                pair: "fg:bg".to_string(),
                ratio,
            });
        }
        Ok(Self {
            color_fg: fg,
            color_bg: bg,
            color_accent: accent,
            contrast_min,
        })
    }

    /// Foreground colour.
    pub fn color_fg(&self) -> &HexColor {
        &self.color_fg
    }

    /// Background colour.
    pub fn color_bg(&self) -> &HexColor {
        &self.color_bg
    }

    /// Accent colour.
    pub fn color_accent(&self) -> &HexColor {
        &self.color_accent
    }

    /// Minimum contrast ratio enforced at construction.
    pub fn contrast_min(&self) -> f64 {
        self.contrast_min
    }
}

/// Typography overrides (font family and scale).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Typography {
    font_family: String,
    /// Base font size in CSS pixels; typical 14..=20.
    base_size_px: u16,
}

impl Typography {
    /// Build a typography record. Font family must be a
    /// non-empty CSS identifier; base size must be 10..=24.
    pub fn new(
        font_family: impl Into<String>,
        base_size_px: u16,
    ) -> Result<Self, ThemeableUiError> {
        let font_family = font_family.into();
        if font_family.is_empty() || font_family.len() > 64 {
            return Err(ThemeableUiError::InvalidFontFamily(font_family));
        }
        if !(10..=24).contains(&base_size_px) {
            return Err(ThemeableUiError::InvalidBaseSize(base_size_px));
        }
        Ok(Self {
            font_family,
            base_size_px,
        })
    }

    /// CSS font-family value.
    pub fn font_family(&self) -> &str {
        &self.font_family
    }

    /// Base font size in px.
    pub fn base_size_px(&self) -> u16 {
        self.base_size_px
    }
}

/// The panel domain an override is bound to. FQDN is normalised
/// to lower-case; wildcards are not permitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelDomain {
    fqdn: String,
}

impl PanelDomain {
    /// Build a panel domain. The FQDN must be a valid DNS name.
    pub fn new(fqdn: impl Into<String>) -> Result<Self, ThemeableUiError> {
        let s: String = fqdn.into();
        let lower = s.trim().to_lowercase();
        if lower.is_empty() || lower.len() > 253 {
            return Err(ThemeableUiError::InvalidPanelDomain(lower));
        }
        if !is_valid_fqdn(&lower) {
            return Err(ThemeableUiError::InvalidPanelDomain(lower));
        }
        Ok(Self { fqdn: lower })
    }

    /// FQDN.
    pub fn fqdn(&self) -> &str {
        &self.fqdn
    }
}

fn is_valid_fqdn(s: &str) -> bool {
    if s.starts_with('.') || s.ends_with('.') {
        return false;
    }
    s.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// The per-account theme override. The `panel_domain` is
/// optional — when `None` the override applies only to the
/// system host when a request is bound to a `UserId` rather
/// than to a host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeOverride {
    owner_id: Uuid,
    brand_name: String,
    logo_path: Option<String>,
    palette: Palette,
    typography: Typography,
    panel_domain: Option<PanelDomain>,
    updated_at: DateTime<Utc>,
}

impl ThemeOverride {
    /// Build a new override. `brand_name` is 1..=64 chars.
    pub fn new(
        owner_id: Uuid,
        brand_name: impl Into<String>,
        palette: Palette,
        typography: Typography,
        panel_domain: Option<PanelDomain>,
    ) -> Result<Self, ThemeableUiError> {
        let brand_name = brand_name.into();
        if brand_name.is_empty() || brand_name.len() > 64 {
            return Err(ThemeableUiError::InvalidBrandName(brand_name));
        }
        Ok(Self {
            owner_id,
            brand_name,
            logo_path: None,
            palette,
            typography,
            panel_domain,
            updated_at: Utc::now(),
        })
    }

    /// Owner user id.
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    /// Brand name.
    pub fn brand_name(&self) -> &str {
        &self.brand_name
    }

    /// Logo path (relative under the owner's branding dir).
    pub fn logo_path(&self) -> Option<&str> {
        self.logo_path.as_deref()
    }

    /// Palette.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Typography.
    pub fn typography(&self) -> &Typography {
        &self.typography
    }

    /// Panel domain.
    pub fn panel_domain(&self) -> Option<&PanelDomain> {
        self.panel_domain.as_ref()
    }

    /// Updated at.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Set the logo path (after a successful upload).
    pub fn set_logo_path(&mut self, path: impl Into<String>) {
        self.logo_path = Some(path.into());
    }

    /// Update the override. Re-checks the brand name length.
    pub fn update(
        &mut self,
        brand_name: impl Into<String>,
        palette: Palette,
        typography: Typography,
        panel_domain: Option<PanelDomain>,
    ) -> Result<(), ThemeableUiError> {
        let brand_name = brand_name.into();
        if brand_name.is_empty() || brand_name.len() > 64 {
            return Err(ThemeableUiError::InvalidBrandName(brand_name));
        }
        self.brand_name = brand_name;
        self.palette = palette;
        self.typography = typography;
        self.panel_domain = panel_domain;
        self.updated_at = Utc::now();
        Ok(())
    }
}

/// Branding scope attached to a hosting plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandingScope {
    /// Reseller mode: the owner can set a per-account override.
    Reseller,
    /// Branding is disabled for this plan.
    #[default]
    Disabled,
    /// System theme only; reserved for the panel's own branding.
    System,
}

impl BrandingScope {
    /// Whether the scope grants branding capability.
    pub fn grants_branding(&self) -> bool {
        matches!(self, BrandingScope::Reseller)
    }

    /// Wire name.
    pub fn as_str(&self) -> &'static str {
        match self {
            BrandingScope::Reseller => "reseller",
            BrandingScope::Disabled => "disabled",
            BrandingScope::System => "system",
        }
    }
}

/// Repository port.
#[async_trait]
pub trait ThemeableUiRepository: Send + Sync + 'static {
    /// Persist or replace the override for `owner_id`.
    async fn upsert_override(&self, override_: &ThemeOverride) -> Result<(), ThemeableUiError>;
    /// Load the override for `owner_id`.
    async fn find_override(
        &self,
        owner_id: Uuid,
    ) -> Result<Option<ThemeOverride>, ThemeableUiError>;
    /// Delete the override for `owner_id`.
    async fn delete_override(&self, owner_id: Uuid) -> Result<(), ThemeableUiError>;
    /// Find the override whose `panel_domain.fqdn` equals `fqdn`.
    async fn find_by_fqdn(&self, fqdn: &str) -> Result<Option<ThemeOverride>, ThemeableUiError>;
}

/// Errors that can occur in the themeable-ui bounded context.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ThemeableUiError {
    /// A colour string is not a valid hex.
    #[error("invalid color: {0}")]
    InvalidColor(String),
    /// Contrast is below the configured minimum.
    #[error("insufficient contrast: pair=`{pair}` ratio={ratio}")]
    InsufficientContrast {
        /// The pair label (e.g. `fg:bg`).
        pair: String,
        /// Measured ratio.
        ratio: f64,
    },
    /// `contrast_min` is out of range.
    #[error("invalid contrast_min: {0}")]
    InvalidContrastMin(f64),
    /// Font family string is invalid.
    #[error("invalid font family: {0}")]
    InvalidFontFamily(String),
    /// Font base size is out of range.
    #[error("invalid base size: {0}")]
    InvalidBaseSize(u16),
    /// Brand name length is invalid.
    #[error("invalid brand name: {0}")]
    InvalidBrandName(String),
    /// Panel domain is malformed.
    #[error("invalid panel domain: {0}")]
    InvalidPanelDomain(String),
    /// An SVG upload was rejected.
    #[error("svg contains forbidden content")]
    SvgContainsForbidden,
    /// Uploaded file is too large.
    #[error("file too large: {0} bytes")]
    FileTooLarge(u64),
    /// Uploaded mime type is not allowed.
    #[error("unsupported mime type: {0}")]
    UnsupportedMime(String),
    /// The user has no branding scope.
    #[error("branding not allowed for plan")]
    BrandingNotAllowed,
    /// The owner has no matching TLS certificate in the ssl cap.
    #[error("no matching certificate for panel domain")]
    NoMatchingCertificate,
    /// Persistence layer failure.
    #[error("themeable-ui persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for ThemeableUiError {
    fn from(error: RepoError) -> Self {
        ThemeableUiError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_parses_3_digit() {
        let c = HexColor::parse("#fff").expect("parse");
        assert_eq!(c.as_hex(), "#ffffff");
    }

    #[test]
    fn hex_color_rejects_bad() {
        let err = HexColor::parse("not-a-color").expect_err("must reject");
        assert!(matches!(err, ThemeableUiError::InvalidColor(_)));
    }

    #[test]
    fn contrast_black_white_is_21() {
        let fg = HexColor::parse("#fff").unwrap();
        let bg = HexColor::parse("#000").unwrap();
        let r = contrast_ratio(&fg, &bg);
        assert!((r - 21.0).abs() < 0.1);
    }

    #[test]
    fn palette_rejects_low_contrast() {
        let fg = HexColor::parse("#aaa").unwrap();
        let bg = HexColor::parse("#bbb").unwrap();
        let accent = HexColor::parse("#000").unwrap();
        let err = Palette::new(fg, bg, accent).expect_err("reject");
        match err {
            ThemeableUiError::InsufficientContrast { pair, ratio } => {
                assert_eq!(pair, "fg:bg");
                assert!(ratio < 4.5);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn palette_accepts_strong_contrast() {
        let fg = HexColor::parse("#fff").unwrap();
        let bg = HexColor::parse("#000").unwrap();
        let accent = HexColor::parse("#0066cc").unwrap();
        Palette::new(fg, bg, accent).expect("accept");
    }

    #[test]
    fn panel_domain_normalises_lowercase_and_rejects_bad() {
        let d = PanelDomain::new("Panel.Acme.COM").expect("parse");
        assert_eq!(d.fqdn(), "panel.acme.com");
        let err = PanelDomain::new("-bad.example.com").expect_err("reject leading dash");
        assert!(matches!(err, ThemeableUiError::InvalidPanelDomain(_)));
    }

    #[test]
    fn typography_rejects_oversize() {
        let err = Typography::new("system-ui", 80).expect_err("reject");
        assert!(matches!(err, ThemeableUiError::InvalidBaseSize(80)));
    }

    #[test]
    fn override_rejects_empty_brand() {
        let palette = Palette::new(
            HexColor::parse("#fff").unwrap(),
            HexColor::parse("#000").unwrap(),
            HexColor::parse("#0066cc").unwrap(),
        )
        .unwrap();
        let typo = Typography::new("system-ui", 16).unwrap();
        let err = ThemeOverride::new(Uuid::new_v4(), "", palette, typo, None).expect_err("reject");
        assert!(matches!(err, ThemeableUiError::InvalidBrandName(_)));
    }

    #[test]
    fn branding_scope_grants_only_reseller() {
        assert!(BrandingScope::Reseller.grants_branding());
        assert!(!BrandingScope::Disabled.grants_branding());
        assert!(!BrandingScope::System.grants_branding());
    }
}
