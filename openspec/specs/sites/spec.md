# sites Specification

## Purpose

The sites bounded context covers site lifecycle, aliases, document
root, and PHP enablement. After this refinement, it also owns
the optional `php_runtime`, `clone_template_id`, and
`instance_origin_id` fields that the per-site-php-runtime and
clone / template-export changes consume.
## Requirements
### Requirement: Per-Site PHP Runtime Reference

The `Site` aggregate SHALL carry an optional `php_runtime: Option<PhpRuntimeRef>` field. When `None`, the site is not associated with a PHP runtime; the per-site-php-runtime bounded context fills the field by calling `Site::set_php_runtime`.

#### Scenario: New site has no runtime

- **WHEN** a new site is created via `Site::new`
- **THEN** `php_runtime` is `None`.

### Requirement: Clone Template and Instance Origin

The `Site` aggregate SHALL carry `clone_template_id: Option<Uuid>` and `instance_origin_id: Option<Uuid>`. Both default to `None` on a fresh site. The `Site::clone` helper builds a draft site with `instance_origin_id = source.id`.

#### Scenario: Clone records origin

- **WHEN** `Site::clone(source, new_id, target_domain, new_owner_id, actor)` is called
- **THEN** the returned site has `instance_origin_id = source.id` and a fresh `id`.

### Requirement: Behaviour Parity

The refinement SHALL introduce the new fields without changing the existing lifecycle (`enable`, `disable`, `change_*`). The follow-on `add-per-site-php-runtime` and `add-site-clone-and-template-export` changes populate the fields and own the install / swap / file copy paths.

#### Scenario: Existing behaviour unchanged

- **WHEN** the refined bounded context is exercised through its public API
- **THEN** behaviour outside the newly added surface is identical to the pre-refinement behaviour.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `SitePhpRuntimeAssigned`, `SiteCloned`, and `SiteTemplateExported` audit events. The bounded context as archived today owns the typed model and the pure-function `Site::clone` helper.

#### Scenario: Clone is audited

- **WHEN** a site is cloned through `Site::clone`
- **THEN** an audit `SiteCloned` event records source and target ids
        without document-root content.

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

