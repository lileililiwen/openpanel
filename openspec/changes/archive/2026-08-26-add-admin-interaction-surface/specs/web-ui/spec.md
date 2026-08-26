## ADDED Requirements

### Requirement: Shell Mounts the Layer Root

The shell SHALL mount a single `<div id="layer-root" aria-live="polite">`
container inside `crates/openpanel-web/src/layout.rs` directly under the
content region. The container SHALL be empty on first render and SHALL
receive all modal, confirm, toast, tip, and load fragments via HTMX OOB
swaps. No route SHALL mount its own modal or toast container.

#### Scenario: Toast appears in the shell-mounted container

- **WHEN** any response carries `HX-Trigger: layer-toast`
- **THEN** the toast fragment is appended to `#layer-root`, and the
  container is the only toast surface on the page

### Requirement: Shell Mounts the Feedback Widget Gate

The shell SHALL include a `<div id="feedback-widget-root" data-account-age-days="{n}">`
container inside `crates/openpanel-web/src/layout.rs`. The
`data-account-age-days` attribute SHALL reflect the authenticated
account's age in days (or be omitted for unauthenticated requests). The
container SHALL have no visible content on first render; the gating
logic — both the server-side `>= 3` age check AND the
`localStorage.openpanel_feedback_seen` check — runs at hydration time and
either renders the widget inside the container or leaves it empty.

#### Scenario: Mature account on fresh browser hydrates widget

- **WHEN** the shell renders for an account with `account_age_days >= 3`
  and `localStorage.openpanel_feedback_seen` is unset
- **THEN** after hydration `#feedback-widget-root` contains the rating
  controls and the optional comment field

#### Scenario: Account younger than 3 days hides widget

- **WHEN** the shell renders for an account with `account_age_days < 3`
- **THEN** after hydration `#feedback-widget-root` is empty

### Requirement: Shell Wires HTMX Toast Hook

The vendored `htmx.min.js` bundle SHALL include (or be accompanied by a
small inline script that adds) an `htmx:afterRequest` listener. When a
response carries the header `HX-Trigger: layer-toast` with a JSON detail
body `{ kind, msg }`, the listener SHALL fetch `GET /layer/toast?kind=...&msg=...`
and append the returned fragment to `#layer-root`. The listener SHALL
NOT swallow errors; on any fetch failure it SHALL log to the JS console
and emit no UI noise.

#### Scenario: After-request hook triggers toast fetch

- **WHEN** any successful form POST returns `200` with `HX-Trigger: layer-toast`
- **THEN** the client listener fetches the toast fragment and appends it
  to `#layer-root`, the toast auto-dismisses after 4 seconds, and no
  additional full-page reload occurs

### Requirement: Layer and Feedback Routes Registered

The composition root SHALL register the new route groups
`GET /layer/{modal,confirm,toast,tip,load}` and `POST /feedback` in the
same web router that already serves `/sites`, `/databases`, etc. The
layer routes SHALL require a valid session and SHALL emit
`Content-Type: text/html; charset=utf-8`.

#### Scenario: Layer routes reachable from any shell page

- **WHEN** any shell page loads and the browser issues `GET /layer/toast?kind=success&msg=Saved`
- **THEN** the route returns `200` with a `<div class="op-toast op-toast--success">...</div>`
  fragment, scoped to the session

### Requirement: CSRF Token in Layer Confirm

Every form rendered by `GET /layer/confirm` SHALL include the per-session
CSRF token as a hidden field, sourced from `crates/openpanel-web/src/csrf.rs`.
The destructive endpoint the form posts to SHALL enforce CSRF per the
existing `web-ui` CSRF requirement.

#### Scenario: Confirm form carries CSRF token

- **WHEN** a user opens `GET /layer/confirm?action=delete-site&id={id}`
- **THEN** the returned fragment contains a `<form>` whose hidden
  `csrf_token` field matches the caller's session token