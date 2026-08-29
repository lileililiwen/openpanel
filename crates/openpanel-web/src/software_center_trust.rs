//! Software Center trust + provenance building blocks.
//!
//! These are the pure, unit-tested models behind the trust-forward catalog
//! experience required by `software-center-trust`: a fail-closed digest-state
//! classifier, an aggregated `TrustView` of an entry's source/signature/
//! dependencies/conflicts/installed state, a compatibility summary, and the
//! exact safe recovery copy for blocked (placeholder/invalid/missing) digests.
//! The install/remove/deploy routes and the preview/execute/rollback flows
//! already exist; this module makes their trust signals explicit and
//! consistent so the UI and the fail-closed gate cannot drift.

use maud::{Markup, html};
use openpanel_app::software_center::{
    CompatibilityReport, PLACEHOLDER_SHA256, StorefrontEntry,
};
use openpanel_domain::software_center::EntryKind;

/// Fail-closed classification of an entry's artifact digest.
///
/// Anything that is not a strictly verified digest is treated as a blocking
/// state when the fail-closed `require_verified_digests` gate is on. System
/// and tool entries have no downloadable artifact, so digest verification is
/// not applicable to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestState {
    /// A real 64-hex SHA-256 pin with no placeholder/invalid value present.
    Verified,
    /// The recovery-seed placeholder sentinel (`PLACEHOLDER_SHA256`).
    Placeholder,
    /// A Web entry whose version(s) carry no artifact pin at all.
    Missing,
    /// A digest string present but malformed (not 64 hex chars).
    Invalid,
    /// No artifact to verify (system/tool entries).
    NotApplicable,
}

impl DigestState {
    /// Human label for the digest state.
    pub fn label(self) -> &'static str {
        match self {
            DigestState::Verified => "Verified digest",
            DigestState::Placeholder => "Placeholder digest",
            DigestState::Missing => "No digest pinned",
            DigestState::Invalid => "Malformed digest",
            DigestState::NotApplicable => "Signed packages",
        }
    }

    /// CSS class fragment for the trust token.
    pub fn class(self) -> &'static str {
        match self {
            DigestState::Verified => "trust--ok",
            DigestState::Placeholder => "trust--blocked",
            DigestState::Missing => "trust--blocked",
            DigestState::Invalid => "trust--blocked",
            DigestState::NotApplicable => "trust--ok",
        }
    }

    /// Whether this state blocks installation under the fail-closed gate.
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            DigestState::Placeholder | DigestState::Missing | DigestState::Invalid
        )
    }
}

/// Classify an entry's digest state fail-closed.
pub fn classify_digest(entry: &StorefrontEntry) -> DigestState {
    if entry.kind != EntryKind::Web {
        return DigestState::NotApplicable;
    }
    let mut saw_artifact = false;
    let mut saw_real = false;
    for version in &entry.versions {
        if let Some(pin) = &version.artifact {
            saw_artifact = true;
            if pin.sha256 == PLACEHOLDER_SHA256 {
                return DigestState::Placeholder;
            }
            if pin.sha256.len() != 64 || !pin.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return DigestState::Invalid;
            }
            saw_real = true;
        }
    }
    if saw_artifact && saw_real {
        DigestState::Verified
    } else {
        DigestState::Missing
    }
}

/// Aggregated trust view of a single catalog entry. Every field is derived
/// from existing catalog data so the UI never re-implements the gate logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustView {
    /// Catalog source URL (from provenance).
    pub source_url: String,
    /// Publisher / developer string.
    pub publisher: String,
    /// SPDX license id.
    pub license: String,
    /// Entry kind label.
    pub kind: &'static str,
    /// Fail-closed digest state.
    pub digest_state: DigestState,
    /// Declared dependency entry ids.
    pub dependencies: Vec<String>,
    /// Declared conflict entry ids.
    pub conflicts: Vec<String>,
    /// Installed-state string.
    pub install_state: String,
}

impl TrustView {
    /// Build a trust view from a storefront entry.
    pub fn from_entry(entry: &StorefrontEntry) -> Self {
        Self {
            source_url: entry.provenance.source_url.clone(),
            publisher: entry.developer.clone(),
            license: entry.license.clone(),
            kind: entry.kind.as_str(),
            digest_state: classify_digest(entry),
            dependencies: entry.dependencies.clone(),
            conflicts: entry.conflicts.clone(),
            install_state: entry.install_state.clone(),
        }
    }

    /// Whether the entry is blocked from install under the fail-closed gate.
    pub fn is_blocked(&self, require_verified_digests: bool) -> bool {
        require_verified_digests && self.digest_state.is_blocking()
    }

    /// Exact, safe recovery copy for a blocked digest, if blocked.
    pub fn recovery_copy(&self, require_verified_digests: bool) -> Option<&'static str> {
        if !self.is_blocked(require_verified_digests) {
            return None;
        }
        Some(blocked_recovery_copy(self.digest_state))
    }
}

/// The exact safe recovery instruction for a blocking digest state. Mirrors
/// the copy already shown on the placeholted detail card so the gate and the
/// UI cannot drift; fail-closed means the recovery never hides the reason.
pub fn blocked_recovery_copy(state: DigestState) -> &'static str {
    match state {
        DigestState::Placeholder => {
            "Run “software refresh” against a remote catalog that pins a real SHA-256. \
             The embedded recovery seed ships a placeholder digest. To opt into lenient \
             air-gapped recovery, set OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false."
        }
        DigestState::Missing => {
            "This Web entry has no pinned artifact. Point the catalog source at a manifest \
             that ships an artifact pin, then refresh the catalog."
        }
        DigestState::Invalid => {
            "The pinned digest is malformed. Re-fetch a valid signed manifest from the \
             catalog source and refresh."
        }
        DigestState::Verified | DigestState::NotApplicable => {
            "This entry is not blocked; no recovery action is required."
        }
    }
}

/// Capability/permission hint derived from the entry kind. Used to surface
/// the host privileges an install will require (no secrets, no credentials).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionSummary {
    /// Human capability labels.
    pub capabilities: Vec<&'static str>,
}

impl PermissionSummary {
    /// Build the capability hint for an entry kind.
    pub fn from_kind(kind: EntryKind) -> Self {
        let capabilities = match kind {
            EntryKind::Web => {
                vec!["Web server", "PHP runtime", "Database"]
            }
            EntryKind::System | EntryKind::Tool => vec!["Host package manager (root)"],
        };
        Self { capabilities }
    }
}

/// Compatibility summary derived from a pre-flight report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilitySummary {
    /// Whether the plan may proceed (no errors).
    pub compatible: bool,
    /// Number of hard errors.
    pub error_count: usize,
    /// Number of warnings.
    pub warning_count: usize,
    /// Human messages (errors first), bounded.
    pub messages: Vec<String>,
}

impl CompatibilitySummary {
    /// Build a summary from a compatibility report.
    pub fn from_report(report: &CompatibilityReport) -> Self {
        let compatible = report.is_compatible();
        let mut messages: Vec<String> = report
            .errors
            .iter()
            .map(|issue| format!("[error] {}: {}", issue.code, issue.message))
            .collect();
        for warning in &report.warnings {
            messages.push(format!("[warn] {}: {}", warning.code, warning.message));
        }
        // Bound the rendered list so a noisy report can't blow up the page.
        messages.truncate(8);
        Self {
            compatible,
            error_count: report.errors.len(),
            warning_count: report.warnings.len(),
            messages,
        }
    }
}

/// Render the digest-state trust token for a card or detail header.
pub fn digest_state_token(state: DigestState) -> Markup {
    html! {
        span class=(format!("trust {}", state.class())) {
            (state.label())
        }
    }
}

/// Render the full trust summary block for an entry detail page. Only
/// non-secret metadata is emitted: source, publisher, license, digest state,
/// capability hint, dependencies, conflicts, and installed state.
pub fn render_trust(entry: &StorefrontEntry, require_verified_digests: bool) -> Markup {
    let view = TrustView::from_entry(entry);
    let permissions = PermissionSummary::from_kind(entry.kind);
    html! {
        section class="detail__trust" aria-label="Trust and provenance" {
            h2 { "Trust & provenance" }
            dl class="detail__metadata" {
                dt { "Source" } dd { (view.source_url) }
                dt { "Publisher" } dd { (view.publisher) }
                dt { "License" } dd { (view.license) }
                dt { "Kind" } dd { (view.kind) }
                dt { "Installed" } dd { (view.install_state) }
            }
            p class="detail__trust-digest" { (digest_state_token(view.digest_state)) }
            @if view.dependencies.is_empty() {
                p { "Dependencies: none" }
            } @else {
                p { "Dependencies: " (view.dependencies.join(", ")) }
            }
            @if view.conflicts.is_empty() {
                p { "Conflicts: none" }
            } @else {
                p { "Conflicts: " (view.conflicts.join(", ")) }
            }
            p { "Requires: " (permissions.capabilities.join(", ")) }
            @if let Some(recovery) = view.recovery_copy(require_verified_digests) {
                div class="banner banner--error" {
                    "Installation blocked: " (recovery)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use openpanel_app::software_center::{
        CompatibilityIssue, CompatibilityReport, StorefrontEntry, StorefrontVersion,
    };
    use openpanel_domain::software_center::{ArtifactPin, Category, EntryKind, Provenance};

    use super::*;

    fn entry(kind: EntryKind, versions: Vec<StorefrontVersion>) -> StorefrontEntry {
        StorefrontEntry {
            id: "wordpress".into(),
            name: "WordPress".into(),
            description: "CMS".into(),
            long_description: String::new(),
            category: Category::Tools,
            kind,
            license: "GPL-2.0".into(),
            developer: "WordPress Foundation".into(),
            homepage: "https://wordpress.org".into(),
            icon: None,
            latest_version: "6.5".into(),
            versions,
            tags: vec![],
            dependencies: vec!["php".into(), "mariadb".into()],
            conflicts: vec![],
            install_state: "available".into(),
            installed_version: None,
            size_bytes: 1024,
            provenance: Provenance {
                source_url: "https://catalog.example/manifest.json".into(),
                manifest_digest: "deadbeef".repeat(8),
                activated_at: "2026-08-29T00:00:00Z".into(),
                entry_count: 1,
                embedded: false,
            },
        }
    }

    fn web_version(sha256: &str) -> StorefrontVersion {
        StorefrontVersion {
            version: "6.5".into(),
            size_bytes: 1024,
            supports_php: vec!["8.3".into()],
            released_at: None,
            changelog_url: None,
            is_latest: true,
            packages: vec![],
            artifact: Some(ArtifactPin {
                url: openpanel_domain::software_center::Homepage::new(
                    "https://wordpress.org/latest.tar.gz",
                )
                .unwrap(),
                sha256: sha256.into(),
                archive_root: "wordpress".into(),
                archive_type: "tar.gz".into(),
                sha1: None,
            }),
        }
    }

    const REAL_SHA: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn verified_web_digest_is_not_blocking() {
        let e = entry(EntryKind::Web, vec![web_version(REAL_SHA)]);
        let view = TrustView::from_entry(&e);
        assert_eq!(view.digest_state, DigestState::Verified);
        assert!(!view.is_blocked(true));
        assert!(view.recovery_copy(true).is_none());
    }

    #[test]
    fn placeholder_digest_blocks_and_copies_recovery() {
        let e = entry(EntryKind::Web, vec![web_version(PLACEHOLDER_SHA256)]);
        let view = TrustView::from_entry(&e);
        assert_eq!(view.digest_state, DigestState::Placeholder);
        assert!(view.is_blocked(true));
        let copy = view.recovery_copy(true).expect("blocked must carry recovery");
        assert!(copy.contains("software refresh"), "recovery must name the action");
        // Lenient mode (gate off) does not block.
        assert!(!view.is_blocked(false));
        assert!(view.recovery_copy(false).is_none());
    }

    #[test]
    fn missing_digest_blocks_web_entry() {
        let mut e = entry(EntryKind::Web, vec![web_version(REAL_SHA)]);
        e.versions[0].artifact = None;
        let view = TrustView::from_entry(&e);
        assert_eq!(view.digest_state, DigestState::Missing);
        assert!(view.is_blocked(true));
    }

    #[test]
    fn malformed_digest_is_invalid_and_blocking() {
        let e = entry(EntryKind::Web, vec![web_version("not-hex!")]);
        let view = TrustView::from_entry(&e);
        assert_eq!(view.digest_state, DigestState::Invalid);
        assert!(view.is_blocked(true));
    }

    #[test]
    fn system_entries_are_not_applicable_to_digest_gate() {
        let e = entry(EntryKind::System, vec![]);
        let view = TrustView::from_entry(&e);
        assert_eq!(view.digest_state, DigestState::NotApplicable);
        assert!(!view.is_blocked(true));
    }

    #[test]
    fn placeholder_wins_over_real_in_multi_version() {
        let mut e = entry(EntryKind::Web, vec![web_version(REAL_SHA)]);
        e.versions.push(web_version(PLACEHOLDER_SHA256));
        assert_eq!(classify_digest(&e), DigestState::Placeholder);
    }

    #[test]
    fn permissions_hint_follows_kind() {
        assert_eq!(
            PermissionSummary::from_kind(EntryKind::Web).capabilities,
            vec!["Web server", "PHP runtime", "Database"]
        );
        assert_eq!(
            PermissionSummary::from_kind(EntryKind::System).capabilities,
            vec!["Host package manager (root)"]
        );
    }

    #[test]
    fn compatibility_summary_surfaces_errors_and_is_bounded() {
        let report = CompatibilityReport {
            errors: vec![
                CompatibilityIssue {
                    code: "php-missing".into(),
                    message: "PHP 8.3 not installed".into(),
                    related: None,
                },
                CompatibilityIssue {
                    code: "db-missing".into(),
                    message: "no database".into(),
                    related: None,
                },
            ],
            warnings: vec![
                CompatibilityIssue {
                    code: "low-disk".into(),
                    message: "disk low".into(),
                    related: None,
                };
                10
            ],
        };
        let summary = CompatibilitySummary::from_report(&report);
        assert!(!summary.compatible);
        assert_eq!(summary.error_count, 2);
        assert_eq!(summary.warning_count, 10);
        // 2 errors + 10 warnings, capped at 8 messages total.
        assert_eq!(summary.messages.len(), 8);
        assert!(summary.messages[0].starts_with("[error]"));
    }

    #[test]
    fn render_trust_emits_source_and_no_secrets() {
        let e = entry(EntryKind::Web, vec![web_version(REAL_SHA)]);
        let out = render_trust(&e, true).into_string();
        assert!(out.contains("https://catalog.example/manifest.json"));
        assert!(out.contains("WordPress Foundation"));
        assert!(out.contains("Verified digest"));
        assert!(out.contains("php, mariadb"));
        // No credential/secret-shaped content should ever appear.
        assert!(!out.to_lowercase().contains("password"));
        assert!(!out.to_lowercase().contains("secret"));
    }
}
