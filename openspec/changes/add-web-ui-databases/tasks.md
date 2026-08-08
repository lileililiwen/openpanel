# Tasks: Add Web UI — Databases

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP). MySQL-gated flows skip when
> `mysql` is absent, matching the existing databases tests.

## 1. Testing — Databases pages

- [ ] 1.1 Unit test in `crates/openpanel-web/src/databases.rs`: the
      list renders one row per database with name, owner, and charset.
- [ ] 1.2 Unit test: the create form renders the required fields
      (site/owner, suffix, charset).
- [ ] 1.3 Unit test: the one-time password panel renders the plaintext
      credential and a rotated-at timestamp; it is never included in
      the list markup.
- [ ] 1.4 Integration test: authenticated `GET /databases` returns the
      table (or empty state).
- [ ] 1.5 Integration test: `POST /databases` with a valid form creates
      the database (mysql-gated) and CSRF validates.
- [ ] 1.6 Integration test: rotating a password renders the new
      plaintext once in the password panel.
- [ ] 1.7 Integration test: reveal renders the plaintext once.
- [ ] 1.8 Integration test: delete requires confirmation and removes
      the row.
- [ ] 1.9 Integration test: CSRF mismatch on a state-changing POST
      returns `403`.
- [ ] 1.10 Integration test: unauthenticated `GET /databases` redirects
      to `/login`.

## 2. Implementation — Databases pages

- [ ] 2.1 Create `crates/openpanel-web/src/databases.rs` with list,
      new, detail, change-password, reveal, and delete handlers wired
      into the web router, behind `WebUser` + CSRF.
- [ ] 2.2 Implement the create flow with inline errors and HTMX list
      swap.
- [ ] 2.3 Implement the one-time password panel (rotation + reveal)
      with the "shown once" transient behavior.
- [ ] 2.4 Implement delete-with-confirmation.

## 3. Validation

- [ ] 3.1 `cargo test --workspace` passes.
- [ ] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [ ] 3.3 Manual smoke: create a database, rotate its password, reveal
      it, and delete it (requires MySQL available).
- [ ] 3.4 Commit + archive via OpenSpec.
