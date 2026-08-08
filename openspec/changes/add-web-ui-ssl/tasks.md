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

- [ ] 1.1 Unit test in `crates/openpanel-web/src/ssl.rs`: the list
      renders one row per certificate with metadata (domain, issuer,
      status, validity, source).
- [ ] 1.2 Unit test: the issue form renders the three sources with the
      correct fields per source and ACME staging default.
- [ ] 1.3 Unit test: the detail row renders the force-https toggle and
      confirmation dialogs for revoke/renew.
- [ ] 1.4 Integration test: no rendered SSL page contains the bytes
      `PRIVATE KEY` (metadata-only guarantee).
- [ ] 1.5 Integration test: self-signed create → certificate appears in
      `GET /ssl` (source `SelfSigned`).
- [ ] 1.6 Integration test: force-https toggle flips the row state.
- [ ] 1.7 Integration test: revoke with confirmation removes the row.
- [ ] 1.8 Integration test: CSRF mismatch on a state-changing POST
      returns `403`.
- [ ] 1.9 Integration test: unauthenticated `GET /ssl` redirects to
      `/login`.

## 2. Implementation — SSL pages

- [ ] 2.1 Create `crates/openpanel-web/src/ssl.rs` with list, new,
      issue, upload, self-signed, renew, revoke, and force-https
      handlers wired into the web router behind `WebUser` + CSRF.
- [ ] 2.2 Implement the three-source issue form + dispatch.
- [ ] 2.3 Implement the force-https HTMX toggle.
- [ ] 2.4 Implement revoke/renew with confirmation dialogs.

## 3. Validation

- [ ] 3.1 `cargo test --workspace` passes.
- [ ] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [ ] 3.3 Manual smoke: issue a self-signed cert, toggle force-HTTPS,
      revoke it (ACME flow gated on network availability).
- [ ] 3.4 Commit + archive via OpenSpec.
