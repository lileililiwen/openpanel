# Add Web UI — Databases

## Why

The databases bounded context provisions MySQL databases end-to-end
(create/list/get/delete/change-password/reveal-password, AES-256-GCM password
encryption at rest). The API and CLI expose it; operators need the same flows
in the browser. This change adds the databases management pages to the web
shell.

## What Changes

- `crates/openpanel-web/src/databases.rs`:
  - `GET /databases` — list (name, owner, character set), actions.
  - `GET /databases/new` + `POST /databases` — create form (site/owner,
    suffix, charset).
  - `GET /databases/{id}` — detail: name, owner, charset, and a password
    management panel.
  - `POST /databases/{id}/password` — rotate password; the new plaintext is
    shown exactly once (matching the API contract).
  - `POST /databases/{id}/reveal` — reveal the current password once.
  - `DELETE /databases/{id}` — delete with confirmation.
- All flows reuse the foundation shell, `WebUser` gating, and CSRF.

## Non-Goals

- Creating databases tied to a user rather than a site (the current model is
  site-owner scoped; follow the service's contract).
- MySQL connection/query UI (no SQL shell) — out of scope.

## Capabilities

### Existing Capabilities

- `web-ui`: adds the databases pages to the shell.
- `databases`: consumed through the `DatabasesService`.
