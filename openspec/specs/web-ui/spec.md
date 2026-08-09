# web-ui Specification

## Purpose
Server-rendered browser UI for the control panel: a pure-Rust HTMX shell
with session authentication (login/logout), per-session CSRF protection,
and embedded static assets, served from the same binary as the JSON API.
## Requirements
### Requirement: Web UI Shell

The panel SHALL serve a browser UI from the same axum server as the JSON API.
Pages SHALL be server-rendered HTML produced by `maud` templates compiled into
the Rust binary. The UI SHALL NOT require a Node.js toolchain, a client-side
framework, or a build step at runtime; the only shipped JavaScript SHALL be a
vendored, embedded `htmx.min.js` used for progressive navigation and form
submission.

#### Scenario: Serving the shell to an authenticated browser

- **WHEN** a browser with a valid `openpanel_session` cookie requests `/`
- **THEN** the server returns an HTML document with a sidebar navigation, a
  topbar showing the logged-in user, and a content region that swaps on
  navigation.

#### Scenario: Unauthenticated browser is redirected

- **WHEN** a browser without a valid session requests a protected page
- **THEN** the server responds with `302 Found` to `/login` rather than a JSON
  `401`.

### Requirement: Login and Logout

The UI SHALL provide `GET /login` (HTML form) and `POST /login`
(credentials). Login SHALL authenticate through the existing identity service
and set the same `openpanel_session` HttpOnly, `SameSite=Lax` cookie the JSON
API issues. Logout SHALL invalidate the session and clear the cookie.

#### Scenario: Successful login

- **WHEN** valid credentials are submitted to `POST /login`
- **THEN** a session is created, the `openpanel_session` cookie is set with
  `HttpOnly` and `Path=/`, and the browser is redirected to the shell.

#### Scenario: Failed login

- **WHEN** invalid credentials are submitted to `POST /login`
- **THEN** the response is `401` and an error is shown in the form; no session
  cookie is set.

#### Scenario: Logout

- **WHEN** an authenticated user submits `POST /logout`
- **THEN** the session is invalidated, the cookie is cleared, and the browser
  is redirected to `/login`.

### Requirement: CSRF Protection

State-changing web handlers SHALL require a per-session CSRF token submitted in
a hidden form field and SHALL reject requests whose token does not match the
session's token with `403 Forbidden`. Every rendered `<form>` SHALL include the
token.

#### Scenario: CSRF mismatch rejected

- **WHEN** a POST carries a missing or incorrect CSRF token
- **THEN** the handler returns `403 Forbidden` and performs no state change.

### Requirement: Static Assets

The UI SHALL serve its static assets (`htmx.min.js`, `app.css`) from bytes
embedded in the binary at `/assets/*`, with correct content types.

#### Scenario: HTMX is served locally

- **WHEN** a browser requests `/assets/htmx.min.js`
- **THEN** the server returns the vendored HTMX script with
  `Content-Type: application/javascript`.

### Requirement: Web Router Mounting

The composition root SHALL mount the web router at `/` alongside the existing
`/api/v1` nest so a single binary serves both the JSON API and the HTML UI.

#### Scenario: Single binary serves both

- **WHEN** the server starts with the web router merged into the API router
- **THEN** requests to `/api/v1/*` return JSON and requests to web routes
  (`/`, `/login`, `/assets/*`) return HTML/static content from the same
  process and port.

### Requirement: Dashboard Landing Page

The `web-ui` shell SHALL render a dashboard at `/` for authenticated users.
The dashboard SHALL show:

- a CPU gauge, a memory gauge, a disk gauge (highest mount percent), and a
  load figure, sourced from the current host snapshot;
- quick-count cards for Sites, Databases, Files, SSL certificates, and Users,
  each linking to its resource page;
- a recent-alerts panel listing the latest alert events, with an empty state
  when no alerts exist.

#### Scenario: Authenticated user opens the dashboard

- **WHEN** a user with a valid session requests `/`
- **THEN** the server renders the dashboard inside the shell with the four
  host gauges, the five quick-count cards, and the recent-alerts panel.

#### Scenario: Host gauges refresh

- **WHEN** a partial request hits `GET /dashboard/gauges`
- **THEN** the server returns an HTML fragment with freshly collected CPU /
  memory / disk / load values that HTMX swaps into place.

### Requirement: Dashboard Empty States

The dashboard SHALL render gracefully when there is nothing to show: zero
counts display `0` on the cards, and the alerts panel shows a "No alerts"
message rather than an error or blank region.

#### Scenario: Fresh install

- **WHEN** a fresh install has no sites, databases, or alerts and the owner
  opens the dashboard
- **THEN** the cards show `0`, the alerts panel shows the empty state, and the
  page renders without error.

### Requirement: Sites List

The `web-ui` SHALL render `GET /sites` inside the shell: a table with one row
per site visible to the caller (domain, status, owner, PHP version), and an
"add site" button for callers permitted to create sites. Non-owner callers
SHALL see only sites they own and SHALL NOT see create/delete actions.

#### Scenario: Listing sites

- **WHEN** an authenticated user requests `/sites`
- **THEN** the server renders the sites table with each visible site's
  domain, status, owner, and PHP version, plus row actions the caller may
  perform.

#### Scenario: Empty site list

- **WHEN** the caller has no sites
- **THEN** the page renders an empty state with an "add site" button (if
  permitted).

### Requirement: Create Site

The UI SHALL provide `GET /sites/new` (form) and `POST /sites` (create). The
form SHALL collect primary domain, aliases, owner, PHP enablement + version,
and an optional document root override. Duplicate-domain and validation errors
SHALL render as an inline alert; successful creation SHALL render the updated
site list.

#### Scenario: Creating a site

- **WHEN** an authorized caller submits a valid site form to `POST /sites`
- **THEN** the site is created (persisted, document root provisioned, nginx
  config applied) and the list re-renders with the new row.

#### Scenario: Duplicate domain

- **WHEN** a caller submits a primary domain that already exists
- **THEN** the handler returns the existing site's domain in an inline error
  alert and creates nothing.

### Requirement: Site Detail and Status

The UI SHALL render `GET /sites/{id}` with the site's domain, aliases,
document root, PHP settings, and status, plus links to its files and SSL
pages. Enable/disable SHALL be available as HTMX actions with CSRF.

#### Scenario: Enabling and disabling

- **WHEN** an authorized caller triggers enable or disable on a site
- **THEN** the site's status flips and the row re-renders with the new
  status.

### Requirement: Delete Site

The UI SHALL render a confirmation before `DELETE /sites/{id}`. Confirmed
deletes remove the site (and its nginx config) and re-render the list.

#### Scenario: Deleting a site

- **WHEN** an authorized caller confirms deletion of a site
- **THEN** the site is deleted and the list re-renders without it.

