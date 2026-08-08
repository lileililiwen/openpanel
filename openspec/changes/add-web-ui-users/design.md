# Design: Add Web UI — Users

## Context

`IdentityService` exposes create_user, list_users, change_role,
disable_user, delete_user, change_password — each enforcing its own role
checks (create/role/disable/delete are Owner-gated in the service and API).
The web layer must mirror that boundary: render the actions only for owners.

## Decisions

### 1. Owner-only page

**Decision**: `GET /users` and all mutation routes require `Role::Owner`; the
`RequireRole` check runs in the web handler (the API already has the pattern).
Non-owners who navigate to `/users` see a "forbidden" message, not the table.

**Rationale**: User administration is an owner capability in the identity
spec; the UI must not leak it to lower roles.

### 2. Last-owner protection

**Decision**: The change-role handler refuses to demote the last remaining
`Owner` (the service enforces this); the UI surfaces the error inline rather
than letting the request 500.

**Rationale**: Prevents an operator from locking themselves out of the panel
by demoting the only owner.

### 3. Passwords never echoed

**Decision**: Create and reset forms collect a new password but never render
it back; responses confirm success without the value. Password fields use
`type="password"` and are excluded from HTMX form persistence.

**Rationale**: The identity spec forbids plaintext passwords in responses/logs
— the UI obeys the same rule.

### 4. Delete confirmation + role change via HTMX

**Decision**: Delete renders a confirmation `<dialog>`; role changes and
disable/enable swap the row in place via HTMX.

**Rationale**: Consistent with the other resource pages' interaction model.

## Security

- All mutation routes validate CSRF.
- Passwords collected but never re-rendered or logged.
- Usernames/emails rendered through `maud` escaping.
- Role gating enforced in the handler (not just hidden buttons).

## Test strategy

- Unit: list rendering, create form (password not echoed), role-change row
  markup, disable/enable/delete actions, forbidden message for non-owners.
- Integration via `TestServer`: owner creates a user → appears in list;
  non-owner gets the forbidden message and no table; role change works and
  last-owner demotion fails inline; disable/enable toggles; delete with
  confirmation; CSRF mismatch 403; unauthenticated redirect.

## Notes

- No new dependencies. Reuses `IdentityService` and the shell.
