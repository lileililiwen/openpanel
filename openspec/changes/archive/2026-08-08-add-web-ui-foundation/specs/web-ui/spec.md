## ADDED Requirements

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
