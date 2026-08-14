# Refine web-ui with audit, accessibility, theming — Tasks

## 1. Testing

- [x] 1.1 Unit tests asserting that the audit stubs return 501
      and no leakage; compile-time scan ensures templates use
      `t(…)` not literal user-visible strings.
- [x] 1.2 Property tests: audit route stub never returns 2xx.
- [x] 1.3 Service tests: stub works under session auth.

## 2. Domain and Application

- [x] 2.1 Implement `audit_route.rs` under
      `crates/openpanel-web/src/`.
- [x] 2.2 Add `tokens.css` and `t.rs` stubs under
      `crates/openpanel-web/src/` and `assets/`.
- [x] 2.3 Add documentation comments in templates pointing to
      the new contracts.

## 3. Adapters and UI

- [x] 3.1 Wire `/audit` and `/audit/events` into
      `openpanel-web`'s router with Owner/Admin role guard.
- [x] 3.2 Update existing templates to call `t(…)` for every
      user-visible string in this change's scope.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (clippy pre-existing; 2 site-staging
      lints fixed in this change).
- [x] 4.3 Smoke-test: `curl /audit` returns 501 with the
      stub header.
- [x] 4.4 Archive with `openspec archive refine-web-ui-with-audit-accessibility-theming`.