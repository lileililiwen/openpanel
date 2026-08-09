## MODIFIED Requirements

### Requirement: Web UI Shell

The panel SHALL serve a browser UI from the same axum server as the JSON API.
Pages SHALL be server-rendered HTML produced by `maud` templates compiled into
the Rust binary. The UI SHALL NOT require a Node.js toolchain, a client-side
framework, or a build step at runtime; the only shipped JavaScript SHALL be a
vendored, embedded `htmx.min.js` used for progressive navigation and form
submission.

The `openpanel-web` crate SHALL provide a responsive shell with top bar,
content region, breadcrumbs, and grouped navigation. Navigation SHALL contain
only registered capabilities the authenticated caller may access, SHALL omit
empty groups, SHALL indicate the active page with `aria-current`, and SHALL
preserve existing route URLs. The groups SHALL be Overview, Hosting,
Operations, Security & Network, and Administration.

#### Scenario: Serving the shell to an authenticated browser

- **WHEN** a browser with a valid `openpanel_session` cookie requests `/`
- **THEN** the server returns an HTML document with grouped sidebar navigation,
  a topbar showing the logged-in user, breadcrumbs, and a content region that
  swaps on navigation

#### Scenario: Unauthenticated browser is redirected

- **WHEN** a browser without a valid session requests a protected page
- **THEN** the server responds with `302 Found` to `/login` rather than a JSON
  `401`

#### Scenario: Owner sees registered capabilities

- **WHEN** an Owner requests a page and Sites, Monitoring, Cron, and Users are registered
- **THEN** the shell renders those links in their defined groups, marks the current link active, and does not render links for unavailable capabilities

#### Scenario: User does not see owner administration

- **WHEN** a User requests the shell
- **THEN** navigation omits owner-only Users and system Settings actions

#### Scenario: Navigation on a narrow viewport

- **WHEN** the shell is displayed at 375 CSS pixels wide
- **THEN** navigation is operable through a keyboard-accessible disclosure control without horizontal page overflow

## ADDED Requirements

### Requirement: Panel Settings

The web UI SHALL provide `/settings`. Owners SHALL see redacted installation information and SHALL update only allowlisted preferences: theme, locale, and timezone. Mutations SHALL validate CSRF and configuration schema, persist atomically, and append an audit event containing field names but no secret values. Other roles SHALL receive `403 Forbidden`.

#### Scenario: Owner updates a preference

- **WHEN** an Owner submits a supported timezone and valid CSRF token
- **THEN** the preference is atomically persisted, the shell uses it on the next request, and an audit event names `timezone`

#### Scenario: Secret configuration is redacted

- **WHEN** an Owner views installation information
- **THEN** database credentials, master keys, session tokens, and private keys are absent from the response

#### Scenario: Unsupported setting is rejected

- **WHEN** an Owner submits a field outside the allowlist
- **THEN** the server returns a validation error and changes no configuration
