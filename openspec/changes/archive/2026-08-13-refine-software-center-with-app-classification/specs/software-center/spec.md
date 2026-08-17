## ADDED Requirements

### Requirement: Component Kind

Every catalog entry, every installed-component row, and every plan item SHALL carry a `kind` enum of `System | Application`. The catalog SHALL maintain separate, typed indices for each kind and SHALL reject cross-kind install requests with `SoftwareError::CrossKindInstallForbidden`. Backwards compatibility: existing `installed_components` rows backfill with `kind='system'`.

#### Scenario: System component install

- **WHEN** a user requests `POST /software/install` with a `system` manifest id
- **THEN** the request is dispatched to the package-manager adapter for system components only; no application install code path is invoked.

#### Scenario: Cross-kind request rejected

- **WHEN** a caller submits an `Application` manifest through `POST /software/install` while the system adapter is selected
- **THEN** the request fails with `CrossKindInstallForbidden` and no subprocess is launched.

#### Scenario: Backfill migration

- **WHEN** the migration runs against a DB that contains pre-existing components
- **THEN** every row's `kind` is `system` and the index is rebuilt.

### Requirement: Web Application Manifest

The system SHALL support a typed `WebApplicationManifest` schema describing an installable web application. Required fields: `kind=application`, `id`, `display_name`, `version`, `category`, `license`, `target_site_type`, `min_php` (when applicable), `requires_db`, `config_overlay_paths`, `artifacts[]` with `url`, `sha256`, `signature_url`. The manifest SHALL be signed by the same Ed25519 panel master key as system manifests. Signature failure MUST reject the manifest.

#### Scenario: Valid manifest signs and verifies

- **WHEN** a manifest signature is valid and expiry not exceeded
- **THEN** the entry appears in the catalog and is selectable.

#### Scenario: Invalid signature rejected

- **WHEN** a manifest's signature does not verify
- **THEN** the catalog refuses the entry, emits an audit `ManifestSignatureRejected`, and the last-known-good manifest stays.

### Requirement: Target Site Type

Each `WebApplicationManifest` SHALL declare a `target_site_type` of `SingleSite | MultiTenant`. The catalog SHALL refuse to install an application whose declared type conflicts with the site it is being installed onto (e.g. `MultiTenant` cannot be installed onto a `Site{ primary_domain: … }` but only onto `MultiTenantSite` aggregates — added by the follow-on site-staging and clone changes).

#### Scenario: SingleSite install

- **WHEN** an application with `target_site_type=single_site` is installed on a `Site`
- **THEN** the install succeeds and a confirmation page is returned at `post_install_url` (relative).

#### Scenario: MultiTenant refused

- **WHEN** an application with `target_site_type=multi_tenant` is installed on a `Site`
- **THEN** the install fails with `SoftwareError::SiteTypeMismatch`.

### Requirement: Catalog UX

The web UI SHALL render system components and web applications on separate tabs. The Applications tab MUST display install preview but MUST NOT yet install until the follow-on `add-web-application-installer` change ships; an explanatory tooltip MUST be shown.

#### Scenario: Web lists both kinds

- **WHEN** an Owner opens `/software`
- **THEN** two tabs are visible: "System" and "Applications"; the Applications tab is informational only.

#### Scenario: API lists both kinds

- **WHEN** a caller requests `GET /software/catalog`
- **THEN** the response is `[{ kind, id, display_name, version, category, license, … }]` with `kind` populated.
