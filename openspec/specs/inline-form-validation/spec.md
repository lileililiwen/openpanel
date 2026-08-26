# inline-form-validation Specification

## Purpose
TBD - created by archiving change add-admin-interaction-surface. Update Purpose after archive.
## Requirements
### Requirement: Per-Field Validation on Blur

The web adapter SHALL validate each form field on the user's first `blur`
event after they have finished interacting with that field, never on every
keystroke. Validation SHALL be triggered via HTMX using
`hx-post="/forms/validate" hx-trigger="blur" hx-target="next .field-error"`
(or equivalent OOB swap), where the response is rendered **inline next to
the offending field**. Validation MUST NOT be presented inside a modal
dialog and MUST NOT be communicated only via a top-of-form summary.

#### Scenario: Invalid email on blur

- **WHEN** a user blurs an `<input name="email">` whose value is not a valid email
- **THEN** the server returns `422` with a partial HTMX fragment carrying the
  inline error message, the error renders next to the email input within 100ms,
  and no modal or top-of-form banner is shown

#### Scenario: Valid field clears its own error

- **WHEN** a user blurs an `<input name="email">` that previously had an error
  and is now valid
- **THEN** the inline error element next to the field is removed via HTMX OOB swap

### Requirement: Server Validation Contract

Every form-handler endpoint under the web adapter SHALL return one of:
- `200 OK` with an HTMX fragment on success (toast trigger or success swap),
- `422 Unprocessable Entity` with a JSON body of shape
  `{ "errors": { "<field>": "<message>", ... } }` and an optional HTMX
  fragment carrying inline error swaps.

The `422` response MUST include the `HX-Trigger` header with the response
header name `form-validation-failed` so client-side handlers can attach
focus management.

#### Scenario: Multi-field error response

- **WHEN** a `POST /sites` request has both an invalid domain and a missing document root
- **THEN** the server returns `422` with `{"errors":{"domain":"...","document_root":"..."}}`
  and inline error fragments targeted at each field

### Requirement: Reusable `ValidationError` View-Model

The web adapter SHALL expose `pub fn render_field_error(field: &str, msg: &str)`
and `pub fn render_field_ok(field: &str)` in `crates/openpanel-web/src/forms.rs`,
and SHALL use these helpers in every form route. No form route SHALL hand-roll
its own error markup.

#### Scenario: Shared helper usage

- **WHEN** any form route in the web adapter returns a validation error
- **THEN** the rendered error markup comes from `render_field_error` and is
  structurally identical across routes (same CSS class, same ARIA attributes)

### Requirement: Shell Mounts Inline-Validation Container

The shell SHALL mount a `<div id="form-errors" hx-swap-oob="true">` root
inside `layout.rs` so that any form route can target it via HTMX OOB swaps
without re-rendering the whole page. The container SHALL have
`aria-live="polite"` so screen readers announce inline errors without
stealing focus.

#### Scenario: OOB swap targets the mounted container

- **WHEN** a form handler returns an inline error fragment with
  `hx-swap-oob="true"` and target `#form-errors`
- **THEN** the fragment is appended to the shell container without a full
  page reload, and assistive technology announces the new error

