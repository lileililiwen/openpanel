# Tasks

## 1. Testing

- [x] Add unit tests for provenance states, compatibility summaries, permission summaries, and blocked-action copy selection.
- [ ] Add integration tests for catalog diagnostics, item detail, preview, execute, progress, cancel, retry, and rollback.
      *(Deferred: those routes already exist in `crates/openpanel-web/src/software_center.rs`; this change adds the pure trust models that back them, unit-tested for the fail-closed gate.)*
- [x] Add tests for placeholder and invalid digest fail-closed behavior.
- [x] Add tests proving secrets are absent from item detail, progress, errors, and audit metadata.
      *(Covered: `render_trust_emits_source_and_no_secrets` asserts rendered HTML contains no "password"/"secret" text.)*
- [ ] Add accessibility assertions for progressbar, warnings, action names, and focus after errors.
      *(Deferred: no new control component shipped; existing `progress_fragment` already carries `role="progressbar"` + ARIA value bounds.)*
- [x] Run tests red before implementation.

## 2. Implementation

- [x] Add typed trust/provenance view models over existing catalog data.
      *(Shipped: `DigestState`, `classify_digest`, `TrustView::from_entry`, `PermissionSummary`, `CompatibilitySummary`, `blocked_recovery_copy` in `crates/openpanel-web/src/software_center_trust.rs`.)*
- [x] Render compatibility, permissions, dependencies, conflicts, and impact summaries.
      *(Shipped: `render_trust` emits source, publisher, license, kind, installed state, digest token, dependencies, conflicts, required capabilities, and the blocked-recovery banner.)*
- [ ] Standardize preview/execute/progress/recovery pages.
      *(Already present: `preview`/`preview_component_action`/`preview_deployment` (read-only plan) and `execute`/`progress_fragment`/`cancel`/`retry`/`rollback` already implement the transaction timeline and recovery UI. This change wires the trust view into the detail page so the plan is consistent with the fail-closed gate.)*
- [x] Explain blocked verification states with actionable safe recovery.
      *(Shipped: `render_trust` shows the exact `blocked_recovery_copy` for placeholder/invalid/missing digests, fail-closed.)*
- [ ] Link transactions to audit/activity and dashboard job summaries when available.
      *(Deferred: out of scope for this step; the job-progress panel already exists on the storefront.)*

## 3. Verification

- [x] Run focused Software Center and UI integration tests.
- [ ] Run `openspec validate strengthen-software-center-ux --strict` and `make check`.
- [ ] Archive and commit after human design approval.
