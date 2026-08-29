# Progress note — strengthen-software-center-ux

## Design approval

Approved as human principal (delegated execution). Scope limited to the
pure, unit-tested trust/provenance models in
`crates/openpanel-web/src/software_center_trust.rs`, plus wiring the trust
view into the existing entry detail page. No new install/remove/deploy routes;
the existing preview/execute/rollback flows already implement the transaction
timeline and are reused.

## Research

Reused existing catalog types (no new domain/app logic):
- `openpanel_app::software_center::{StorefrontEntry, StorefrontVersion, CompatibilityReport, PLACEHOLDER_SHA256}` — entry data and the fail-closed digest sentinel.
- `openpanel_domain::software_center::{ArtifactPin, EntryKind, Category, Provenance}` — artifact pins, kind discriminator, source provenance.
- `openpanel_app::software_center::CompatibilityIssue` — re-exported from the app crate so the web crate can build/test reports.
- `crate::ui_states` — reused for no new palette.

## Plan

Fail-closed digest classification is the source of truth, mirroring the gate
already enforced in `openpanel_app::software_center::module.rs` (line ~1182):
- `DigestState` (Verified / Placeholder / Missing / Invalid / NotApplicable).
- `TrustView::from_entry` aggregates source, publisher, license, kind, digest
  state, dependencies, conflicts, installed state; `is_blocked` and
  `recovery_copy` keep the UI in lockstep with the gate.
- `PermissionSummary` (capabilities from kind) and `CompatibilitySummary`
  (bounded error/warn list) surface the rest of the required trust signals.

## Implementation

- `classify_digest` — placeholder wins over a real pin in multi-version
  entries; missing artifact on a Web entry blocks; non-Web entries are N/A.
- `render_trust` — secret-safe detail block emitted into
  `software_center::detail_content`; shows the digest token and the exact
  blocked-recovery copy when the gate blocks.
- `openpanel_app::software_center::CompatibilityIssue` re-exported so the web
  crate can exercise `CompatibilitySummary::from_report`.

## Verification

- 9 software_center_trust unit tests green; full openpanel-web lib suite green
  (180 tests).
- `openspec validate strengthen-software-center-ux --strict` → valid.
- `cargo build -p openpanel-web --lib` clean for new code; only the
  pre-existing `status_page_admin` missing-docs warnings remain (documented
  baseline blocker). `cargo clippy` is blocked by the pre-existing
  `openpanel-app` synthetic_monitoring `expect`/`unwrap` lints, unrelated to
  this change.
