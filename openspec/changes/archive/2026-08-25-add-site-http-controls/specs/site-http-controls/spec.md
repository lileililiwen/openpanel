## ADDED Requirements

### Requirement: Custom Error Pages

The system SHALL let an authorised caller override the response body
served for a given HTTP status class (e.g. 404, 500) on a site by
pointing at a document inside that site's root. Overrides SHALL be
rejected when the document path escapes the site root.

#### Scenario: Override served

- **WHEN** an Owner sets a 404 override to `/errors/404.html` and a
        client requests a missing path
- **THEN** nginx serves the override document with status 404.

#### Scenario: Path outside site rejected

- **WHEN** the override document resolves outside the site root
- **THEN** the request fails with `SiteHttpError::PathOutsideSite` and
        nothing is persisted.

### Requirement: Redirect Rules

The system SHALL let an authorised caller manage an ordered list of
per-site redirect rules mapping a source path prefix to a destination
with status 301, 302, 307, or 308. A rule whose source prefix would
loop against an existing rule's destination SHALL be rejected.

#### Scenario: Prefix redirected

- **WHEN** a rule maps `/old` → `/new` with 301 and a client requests
        `/old/page`
- **THEN** the client receives 301 with `Location: /new/page`.

#### Scenario: Loop rejected

- **WHEN** a new rule's source prefix equals the destination prefix of
        an existing rule
- **THEN** the mutation fails with `SiteHttpError::RedirectLoop`.

### Requirement: Protected Directories

The system SHALL let an authorised caller protect a path prefix with
HTTP basic auth backed by per-directory accounts. Account passwords
SHALL be stored only as bcrypt hashes; plaintext passwords and hashes
SHALL never appear in logs, API responses, or rendered vhost text.

#### Scenario: Auth required

- **WHEN** a client requests a path inside a protected directory
        without credentials
- **THEN** the response is 401 with the configured realm.

#### Scenario: Secret hygiene

- **WHEN** any account is created or listed
- **THEN** storage and responses contain the bcrypt hash only, never
        the plaintext, and the rendered vhost references the htpasswd
        file path rather than any credential material.

### Requirement: Hotlink Protection

The system SHALL let an authorised caller enable referer-based hotlink
protection with an allow-list of referers and a default deny or allow
policy for unmatched referers.

#### Scenario: Foreign referer blocked

- **WHEN** hotlink protection is enabled with default deny and a
        request carries a referer not on the allow-list
- **THEN** the response is 403.

#### Scenario: Allowed referer passes

- **WHEN** the referer matches an allow-list entry
- **THEN** the asset is served normally.

### Requirement: Per-Site Client IP Rules

The system SHALL let an authorised caller manage ordered per-site
client IP allow/deny CIDR rules evaluated before application routing.

#### Scenario: Denied CIDR blocked

- **WHEN** a client IP falls inside a Deny rule's CIDR
- **THEN** the response is 403 regardless of other controls.

#### Scenario: Allow wins over default

- **WHEN** rules are `[Allow 203.0.113.0/24]` and the client IP is
        `203.0.113.9`
- **THEN** the request proceeds to the application.

### Requirement: MIME Type Overrides

The system SHALL let an authorised caller override the MIME type
served for a file extension on a site.

#### Scenario: Override applied

- **WHEN** `.webmanifest` is mapped to
        `application/manifest+json` and a client fetches
        `/app.webmanifest`
- **THEN** the response `Content-Type` is the mapped type.

### Requirement: Directory Index Policy

The system SHALL let an authorised caller configure the ordered index
file list and enable or disable directory autoindexing per site.

#### Scenario: Custom index honoured

- **WHEN** the index order is `["index.php", "index.html"]` and a
        client requests `/`
- **THEN** nginx serves `index.php` when present.

#### Scenario: Autoindex toggle

- **WHEN** autoindex is disabled and a directory has no index file
- **THEN** the response is 403; when enabled, a listing is served.

### Requirement: Deterministic Rendering and Audit

Compiled nginx snippets for these controls SHALL be a pure function of
the stored controls (byte-stable for equal input), and every mutation
SHALL emit an audit event naming the site and section touched.

#### Scenario: Stable output

- **WHEN** the same controls are compiled twice
- **THEN** the emitted vhost fragments are byte-identical.

#### Scenario: Mutation audited

- **WHEN** any control section is created, updated, or deleted
- **THEN** an audit event records `{site_id, section, actor}` with no
        secret material.
