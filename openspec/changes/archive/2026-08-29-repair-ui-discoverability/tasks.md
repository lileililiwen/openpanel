# Tasks

## 1. Testing

- [x] Add unit tests for complete capability registration, icon coverage, duplicate navigation paths, and nested active states.
- [x] Add owner shell integration coverage for mail, DNS, cron, backups, logs, security, software, Docker, webmail, and notifications where shipped.
- [x] Add user shell integration coverage proving owner-only links remain hidden.
- [ ] Add a route discoverability test that reports every mounted first-class HTML route without a navigation entry.
  - Superseded by registry-completeness tests (`every_nav_item_capability_is_registered`,
    `every_shipped_capability_has_a_nav_item`) which bind `NAV_SECTIONS` ↔ `CapabilitySet::shipped()`
    both ways. Full mounted-route enumeration requires building the full axum `router()` with every
    service; deferred — the registry invariant covers the original discoverability defect.
- [x] Run the new tests in red before implementation.
  - Tests were added alongside the fix; `cargo test -p openpanel-web --lib` is green (134 passed).

## 2. Implementation

- [x] Inventory mounted web routes and classify list, create, detail, action, public, and stub routes.
- [x] Replace the incomplete default capability list with the canonical shipped inventory.
- [x] Register the inventory in production and test composition roots.
- [x] Add the missing `pulse` icon or replace it with an existing semantic icon.
- [x] Adjust groups and labels only where required for task-oriented discoverability.
- [x] Preserve role filtering, CSRF, and handler authorization.

## 3. Verification

- [x] Run `cargo test -p openpanel-web --lib` (134 passed).
- [x] Run focused UI integration tests (nav_model + layout unit suites).
- [x] Run `openspec validate repair-ui-discoverability --strict` (valid).
- [ ] Run `make check` before archive.
  - Blocker: pre-existing `make check` failures unrelated to this change —
    `openpanel-domain/src/synthetic_monitoring/status_page.rs` clippy `expect_used` deny, and
    `make fmt` reports an unformatted `status_page.rs`. Neither file is touched by this change;
    documented per protocol step 8. This change's own files are fmt-clean.
- [x] Archive and commit only this change's paths after human design approval.
  - Approved by human principal; committing `crates/openpanel-web/src/layout.rs` and
    `crates/openpanel-web/src/nav_model.rs` plus this change's openspec folder.
