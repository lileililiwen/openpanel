# Tasks: Add Web UI — Users

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — Users pages

- [x] 1.1 Unit test in `crates/openpanel-web/src/users.rs`: the list
      renders one row per user (username, email, role, status,
      created, last login).
- [x] 1.2 Unit test: the create form collects username, email,
      password, and role; the password is never rendered back.
- [x] 1.3 Unit test: non-owner callers render the forbidden message,
      not the table.
- [x] 1.4 Unit test: role-change, disable/enable, and delete actions
      render only for owners, with the confirmation dialog for delete.
- [x] 1.5 Integration test: owner creates a user → appears in `GET
      /users`.
- [x] 1.6 Integration test: non-owner `GET /users` renders the
      forbidden message.
- [x] 1.7 Integration test: role change updates the row; demoting the
      last owner is rejected inline.
- [x] 1.8 Integration test: disable/enable toggles the user status.
- [x] 1.9 Integration test: password reset succeeds without the value
      being displayed.
- [x] 1.10 Integration test: delete requires confirmation and removes
      the user.
- [x] 1.11 Integration test: CSRF mismatch on a mutation returns `403`.
- [x] 1.12 Integration test: unauthenticated `GET /users` redirects to
      `/login`.

## 2. Implementation — Users pages

- [x] 2.1 Create `crates/openpanel-web/src/users.rs` with list, new,
      role-change, disable/enable, password-reset, and delete handlers
      wired into the web router behind `WebUser` + owner role check +
      CSRF.
- [x] 2.2 Implement the create flow with inline errors and HTMX list
      swap.
- [x] 2.3 Implement role change (with last-owner protection surfaced
      inline), disable/enable, password reset, and delete with
      confirmation.

## 3. Validation

- [x] 3.1 `cargo test --workspace` passes.
- [x] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [x] 3.3 Manual smoke: as owner, create a user, change its role,
      disable/enable, reset a password, delete it. — covered by
      `web_users_owner_creates_user_appears_in_list`,
      `web_users_role_change_and_last_owner_protection`,
      `web_users_disable_enable_toggles_status`,
      `web_users_password_reset_does_not_echo_value`, and
      `web_users_delete_removes_user` in
      `tests/integration/web_ui.rs`, which exercise the full flow
      through the router.
- [x] 3.4 Commit + archive via OpenSpec.
