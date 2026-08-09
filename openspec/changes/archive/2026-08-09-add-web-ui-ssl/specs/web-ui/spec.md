## ADDED Requirements

### Requirement: Certificate List

The `web-ui` SHALL render `GET /ssl` inside the shell: a table with one row
per certificate (domain, issuer, status, validity, source) and an "issue
certificate" action. Detail and list pages SHALL render metadata only and
SHALL NOT contain private-key material in any form.

#### Scenario: Listing certificates

- **WHEN** an authenticated user requests `/ssl`
- **THEN** the server renders the certificate table with metadata only.

#### Scenario: Metadata-only guarantee

- **WHEN** any SSL page is rendered
- **THEN** its HTML SHALL NOT contain the bytes `PRIVATE KEY`.

### Requirement: Issue Certificate

The UI SHALL provide `GET /ssl/new` (form) and create actions for the three
sources: ACME issuance (staging by default, production opt-in), manual PEM
upload, and self-signed generation.

#### Scenario: Self-signed issuance

- **WHEN** an authorized user generates a self-signed certificate for a
  domain
- **THEN** the certificate appears in the list with source `SelfSigned` and
  metadata rendered.

#### Scenario: ACME staging default

- **WHEN** a user issues via ACME without selecting production
- **THEN** the request targets the staging directory and the resulting
  certificate is recorded with source `Acme`.

### Requirement: Certificate Actions

The UI SHALL provide renew (`POST /ssl/{domain}/renew`), force-HTTPS toggle
(`PATCH /ssl/{domain}/force-https`), and revoke+delete (`POST
/ssl/{domain}/revoke`) with confirmation. Revoke and renew SHALL require a
confirmation before dispatch.

#### Scenario: Force-HTTPS toggle

- **WHEN** an authorized user flips the force-https switch for a domain
- **THEN** the per-site 301 redirect state updates and the row re-renders
  with the new state.

#### Scenario: Confirmed revoke

- **WHEN** an authorized user confirms revocation for a domain
- **THEN** the certificate is revoked and deleted and the list re-renders
  without it.
