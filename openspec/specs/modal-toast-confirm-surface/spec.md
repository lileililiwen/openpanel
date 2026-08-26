# modal-toast-confirm-surface Specification

## Purpose
TBD - created by archiving change add-admin-interaction-surface. Update Purpose after archive.
## Requirements
### Requirement: Single Layer Surface for All Overlays

The web adapter SHALL provide a single `layer` module at
`crates/openpanel-web/src/layer.rs` that owns every overlay on the panel:
modal dialogs (`/layer/modal`), confirmation prompts (`/layer/confirm`),
toasts (`/layer/toast`), tooltips (`/layer/tip`), and blocking load
overlays (`/layer/load`). No route SHALL hand-roll its own overlay markup.

#### Scenario: All overlay routes are served by layer.rs

- **WHEN** the panel boots and `GET /layer/toast?kind=success&msg=Saved`,
  `GET /layer/modal?id=...`, `GET /layer/confirm?action=...&id=...&csrf_token=...`,
  `GET /layer/tip?id=...`, and `GET /layer/load?label=...` are each requested
- **THEN** every response is `200` with a `Content-Type: text/html` fragment
  and is served from the same `layer` module

### Requirement: Modals Reserved for Destructive Actions

A modal SHALL be used only to confirm a destructive action — `delete`,
`remove`, `stop`, `restart`, `terminate`, `purge`, `rotate-secret`,
`disable-2fa`, or `revoke`. Modals MUST NOT be used to surface
validation errors, routine success, or any non-destructive informational
content.

#### Scenario: Delete site opens confirm modal

- **WHEN** a user clicks the row action `Delete` on a site
- **THEN** the shell opens `GET /layer/confirm?action=delete-site&id={id}` and
  renders the destructive confirmation, scoped to the destructive action set

#### Scenario: Validation error does not open a modal

- **WHEN** a form field fails validation
- **THEN** no modal is opened; the error renders inline next to the field

### Requirement: Toast for Routine Feedback

The web adapter SHALL emit non-blocking toasts (`/layer/toast`) for routine
success feedback such as `Saved`, `Created`, `Rotated`, `Updated`,
`Restored`, `Sent`. Toasts SHALL auto-dismiss after 4 seconds and SHALL
be dismissable via click. Each successful form POST SHALL trigger exactly
one toast with `kind=success` and `msg=<human readable outcome>`.

#### Scenario: Successful site create fires toast

- **WHEN** `POST /sites` returns `200` with a success fragment
- **THEN** the response includes `HX-Trigger: layer-toast` with detail
  `{ "kind": "success", "msg": "Site created" }`, and the shell renders
  a dismissable toast that auto-clears in 4s

### Requirement: Confirm Endpoint Requires CSRF

`GET /layer/confirm` SHALL accept `id`, `action`, and `csrf_token` query
parameters; the destructive action it links to SHALL refuse execution
without a valid CSRF token. The fragment returned MUST contain a single
form whose `action` is `POST` and whose body includes the CSRF token.

#### Scenario: Confirm without CSRF token is rejected

- **WHEN** a user submits the destructive form embedded in `/layer/confirm`
  with a missing or invalid CSRF token
- **THEN** the destructive endpoint returns `403 Forbidden` and no state
  changes; the modal closes and a `layer-toast` with `kind=error` is emitted

### Requirement: Migrate Existing Destructive Actions

Every destructive endpoint in `sites.rs`, `databases.rs`, `cron.rs`,
`backups.rs`, `ssl.rs`, `files.rs`, `users.rs`, `dns.rs`, `ftp.rs`,
`mail.rs` SHALL route its delete/stop/restart/remove action through
`/layer/confirm` rather than rendering a custom prompt or accepting a
plain `DELETE` from a list row.

#### Scenario: All delete row-actions use confirm

- **WHEN** a user opens any list view (sites, databases, cron jobs,
  backups, SSL, files, users, DNS, FTP, mail)
- **THEN** each row's delete action renders an `hx-get="/layer/confirm?action=..."`
  link and never a plain destructive button

### Requirement: Shell Wires HTMX Toast Hook

The shell's vendored `htmx.min.js` SHALL be augmented with a
`htmx:afterRequest` listener that, when a response includes the
`HX-Trigger: layer-toast` header, fetches `GET /layer/toast` and appends
the returned fragment to the `<div id="layer-root">` container mounted by
the shell. No route SHALL emit toast markup directly.

#### Scenario: HX-Trigger layer-toast renders toast

- **WHEN** any response carries `HX-Trigger: layer-toast`
- **THEN** the client-side handler fetches `/layer/toast`, appends it to
  `#layer-root`, and the toast auto-dismisses after 4 seconds

