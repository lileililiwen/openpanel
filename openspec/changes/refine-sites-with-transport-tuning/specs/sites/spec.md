## ADDED Requirements

### Requirement: Per-Site Transport Policy

Each site SHALL carry a validated `TransportPolicy` governing HTTP/3
QUIC enablement, minimum TLS version, HSTS header, compression engine
and level, and request body size cap. Rendering SHALL be a pure
function of the stored policy, and the default policy SHALL produce
output byte-identical to the pre-policy renderer.

#### Scenario: Default is zero-diff

- **WHEN** a site that never edited its transport policy is rendered
- **THEN** the vhost bytes match the legacy fixed output exactly.

#### Scenario: HTTP/3 toggle

- **WHEN** an Owner enables HTTP/3 for a site
- **THEN** the vhost gains a QUIC listener and Alt-Svc header, and
        disabling removes both.

### Requirement: Policy Validation

The domain SHALL reject invalid policies at construction: TLS floors
below 1.2, HSTS preload with max-age under one year, compression
levels outside 1–9, and body caps outside 1 KiB–10 GiB.

#### Scenario: Invalid preload rejected

- **WHEN** a policy sets preload with max-age 86400
- **THEN** construction fails and nothing is persisted.

### Requirement: Transport Surfaces

Owners SHALL read and update the transport policy via API, CLI, and
web; every mutation SHALL be audited with the changed fields only.

#### Scenario: Round-trip over HTTP

- **WHEN** an Owner PUTs a valid policy then GETs it
- **THEN** the returned policy equals the stored one and an audit
          event lists the changed fields.
