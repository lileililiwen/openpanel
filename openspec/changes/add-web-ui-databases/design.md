# Design: Add Web UI — Databases

## Context

`DatabasesService` exposes create/list/get/delete/change-password/
reveal-password. Passwords are stored as AES-256-GCM ciphertext; the service
returns plaintext **exactly once** (change/reveal) and the API never returns
it in list/get responses. The web pages must honor the same "plaintext shown
once" contract.

## Decisions

### 1. Plaintext password shown exactly once

**Decision**: The password rotation and reveal flows render the plaintext
credential in a one-time panel (`password + username + host`) that the page
does not persist. After render, the handler does not retain the plaintext; a
page refresh shows only "password rotated at <time>".

**Rationale**: Matches the backend's security posture (see the API route
`POST /databases/{id}/password`, which returns plaintext once). Showing the
credential in the list would defeat the encryption-at-rest design.

### 2. HTMX for status feedback, full page for password panels

**Decision**: List mutations (create/delete) swap the table in place.
Password change/reveal returns a dedicated one-time view region
(`#password-panel`) so the plaintext is transient and easy to clear.

**Rationale**: A transient panel makes the "shown once" guarantee obvious and
testable, and keeps the plaintext out of the persistent table DOM.

### 3. RBAC scoping

**Decision**: `list_databases(caller)` and `create_database(caller, ...)`
already scope by role/owner; the UI renders create/delete/change-password
actions only when the caller may perform them.

**Rationale**: Mirror the service's authorization; never render actions the
service would reject.

### 4. Delete confirmation

**Decision**: `DELETE /databases/{id}` renders a confirmation `<dialog>`
first, mirroring the sites delete flow.

**Rationale**: Dropping a database is destructive and irreversible.

## Security

- Plaintext credentials appear only in the transient password panel; never in
  the list, detail metadata, logs, or persisted HTML.
- All state-changing routes validate the CSRF token.
- Database names/owners rendered via `maud` escaping.

## Test strategy

- Unit: list rendering, create form fields, one-time password panel markup.
- Integration via `TestServer`: create → listed; change password shows the
  new plaintext once; reveal shows it once; delete with confirmation removes
  the row; CSRF mismatch 403; unauthenticated redirect; forbidden role hides
  actions.

## Notes

- No new dependencies. Reuses `DatabasesService` and the shell.
- Database creation shells out to MySQL; the existing integration tests skip
  when `mysql` is absent — the web tests inherit that gating.
