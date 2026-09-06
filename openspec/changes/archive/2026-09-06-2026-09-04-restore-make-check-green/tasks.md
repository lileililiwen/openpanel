## 1. Testing

- [x] 1.1 Confirm `cargo check --workspace --all-targets` compiles after
      the audit split; this is the check that proves the four
      `AuditService` doubles satisfy the trait they already declared.
- [x] 1.2 Confirm `make file-length` exits zero with all three former
      violations now under the 1000-line hard limit.
- [x] 1.3 Confirm the three `web_ui_styling` assertions that failed on
      `/audit` now pass: global form class, paired labels, and no ad-hoc
      class tokens.

## 2. Implementation

- [x] 2.1 Split `app/src/sites/nginx.rs` (1023) into
      `sites/nginx/mod.rs` and `sites/nginx/tests.rs`.
- [x] 2.2 Split `web/src/dashboard.rs` (1051) into
      `web/dashboard/mod.rs` and `web/dashboard/tests.rs`.
- [x] 2.3 Split `core/src/audit/mod.rs` (1608) into `mod.rs`,
      `action.rs`, `sqlite.rs`, `cursor.rs`, `redaction.rs`, and
      `tests.rs`; delete `strings.rs`; re-export the moved items so
      `openpanel_core::audit::*` is unchanged for callers.
- [x] 2.4 Add the missing `query` implementation to the four app-layer
      audit test doubles.
- [x] 2.5 Replace the `unwrap()` in `render_event_row` with
      `if let Some(meta) = view.metadata.as_object().filter(..)`.
- [x] 2.6 Remove the ad-hoc `audit-filters` class from the `/audit`
      filter form and wrap each visible control in a `<label>`, dropping
      placeholders that only repeated the new label text.

## 3. Verification

- [x] 3.1 Run `cargo clippy --workspace --all-targets` and confirm zero
      warnings.
- [x] 3.2 Run `make test` and confirm the full suite passes.
- [x] 3.3 Run `make agent-governance`, `make governance-contract`, and
      `make test-gates`; confirm no governance requirement was weakened.
- [x] 3.4 Run `make check` end to end and confirm it reports all quality
      checks passed.
- [x] 3.5 Obtain human approval of `design.md`.
