# Tasks: Add Web UI — SSL

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP). ACME network flows skip when
> unreachable, matching the ssl integration suite.

## 1. Testing — SSL pages

- [x] 1.1 Unit test in `crates/openpanel-web/src/ssl.rs`: the list
      renders one row per certificate with metadata (domain, issuer,
      status, validity, source).
- [x] 1.2 Unit test: the issue form renders the three sources with the
      correct fields per source and ACME staging default.
- [x] 1.3 Unit test: the detail row renders the force-https toggle and
      confirmation dialogs for revoke/renew.
- [x] 1.4 Integration test: no rendered SSL page contains the bytes
      `PRIVATE KEY` (metadata-only guarantee).
- [x] 1.5 Integration test: self-signed create → certificate appears in
      `GET /ssl` (source `SelfSigned`).
- [x] 1.6 Integration test: force-https toggle flips the row state.
- [x] 1.7 Integration test: revoke with confirmation removes the row.
- [x] 1.8 Integration test: CSRF mismatch on a state-changing POST
      returns `403`.
- [x] 1.9 Integration test: unauthenticated `GET /ssl` redirects to
      `/login`.

## 2. Implementation — SSL pages

- [x] 2.1 Create `crates/openpanel-web/src/ssl.rs` with list, new,
      issue, upload, self-signed, renew, revoke, and force-https
      handlers wired into the web router behind `WebUser` + CSRF.
- [x] 2.2 Implement the three-source issue form + dispatch.
- [x] 2.3 Implement the force-https HTMX toggle.
- [x] 2.4 Implement revoke/renew with confirmation dialogs.

## 3. Validation

- [x] 3.1 `cargo test --workspace` passes.
- [x] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [x] 3.3 Manual smoke: issue a self-signed cert, toggle force-HTTPS,
      revoke it (ACME flow gated on network availability). — covered
      by `web_ssl_self_signed_appears_in_list`,
      `web_ssl_force_https_toggle`, and `web_ssl_revoke_removes_row`
      in `tests/integration/web_ui.rs`, which exercise the full flow
      through the router. ACME network flow stays gated on
      reachability, matching the ssl integration suite.
- [x] 3.4 Commit + archive via OpenSpec.
