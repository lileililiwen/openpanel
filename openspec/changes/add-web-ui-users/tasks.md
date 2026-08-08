# Tasks: Add Web UI — Users

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — Users pages

- [ ] 1.1 Unit test in `crates/openpanel-web/src/users.rs`: the list
      renders one row per user (username, email, role, status,
      created, last login).
- [ ] 1.2 Unit test: the create form collects username, email,
      password, and role; the password is never rendered back.
- [ ] 1.3 Unit test: non-owner callers render the forbidden message,
      not the table.
- [ ] 1.4 Unit test: role-change, disable/enable, and delete actions
      render only for owners, with the confirmation dialog for delete.
- [ ] 1.5 Integration test: owner creates a user → appears in `GET
      /users`.
- [ ] 1.6 Integration test: non-owner `GET /users` renders the
      forbidden message.
- [ ] 1.7 Integration test: role change updates the row; demoting the
      last owner is rejected inline.
- [ ] 1.8 Integration test: disable/enable toggles the user status.
- [ ] 1.9 Integration test: password reset succeeds without the value
      being displayed.
- [ ] 1.10 Integration test: delete requires confirmation and removes
      the user.
- [ ] 1.11 Integration test: CSRF mismatch on a mutation returns `403`.
- [ ] 1.12 Integration test: unauthenticated `GET /users` redirects to
      `/login`.

## 2. Implementation — Users pages

- [ ] 2.1 Create `crates/openpanel-web/src/users.rs` with list, new,
      role-change, disable/enable, password-reset, and delete handlers
      wired into the web router behind `WebUser` + owner role check +
      CSRF.
- [ ] 2.2 Implement the create flow with inline errors and HTMX list
      swap.
- [ ] 2.3 Implement role change (with last-owner protection surfaced
      inline), disable/enable, password reset, and delete with
      confirmation.

## 3. Validation

- [ ] 3.1 `cargo test --workspace` passes.
- [ ] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [ ] 3.3 Manual smoke: as owner, create a user, change its role,
      disable/enable, reset a password, delete it.
- [ ] 3.4 Commit + archive via OpenSpec.
