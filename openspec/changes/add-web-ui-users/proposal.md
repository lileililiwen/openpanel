# Add Web UI — Users

## Why

Identity is the oldest bounded context: users, roles (Owner/Admin/User),
argon2id password hashing, session lifecycle. The CLI can create users but
only the owner can manage the panel's accounts. A hosting panel without a
user-management screen is incomplete — every panel in this class has one.
This change adds the user pages to the web shell, scoped to what the
authenticated caller may do.

## What Changes

- `crates/openpanel-web/src/users.rs`:
  - `GET /users` — user list (username, email, role, status, created,
    last login). Owner-only.
  - `GET /users/new` + `POST /users` — create user (username, email,
    password, role).
  - `PATCH /users/{id}/role` — change role (Owner only, cannot demote the
    last owner).
  - `POST /users/{id}/disable` / `/enable` — status toggle.
  - `POST /users/{id}/password` — reset a user's password.
  - `DELETE /users/{id}` — delete with confirmation (Owner only).
- All handlers run through the foundation shell, `WebUser` gating, and CSRF,
  and honor the service's role checks (Owner-only actions render only for
  owners).

## Non-Goals

- Self-service profile/settings page (change own password in a follow-up).
- SSO / OAuth / MFA.
- Session management UI (list/logout active sessions) — follow-up.

## Capabilities

### Existing Capabilities

- `web-ui`: adds the user management pages to the shell.
- `identity`: consumed through the `IdentityService` (create_user,
  list_users, change_role, disable_user, delete_user, change_password).
