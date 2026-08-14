# plugin-extension-framework Specification

## Purpose
TBD - created by archiving change 2026-08-14-refine-plugin-extension-framework-with-marketplace. Update Purpose after archive.
## Requirements
### Requirement: Remote Catalog Discovery

The system SHALL provide a curated remote plugin catalog with a
local cache. On discovery the panel SHALL verify each entry's
publisher signature against the configured marketplace CA; entries
that fail verification SHALL be flagged and MUST NOT be offered for
install.

#### Scenario: Catalog lists only CA-chained publishers

- **WHEN** the panel fetches the marketplace catalog
- **THEN** only entries whose publisher signature chains to the
        marketplace CA are presented as installable; unverified
        entries are flagged `unverified`.

#### Scenario: Tampered entry rejected

- **WHEN** a catalog entry's manifest is tampered after signing
- **THEN** the entry fails signature verification and is excluded
        from installable results.

### Requirement: Marketplace Install Reuses Framework Protocol

`POST /marketplace/plugins/{id}/install` SHALL resolve the pinned
`manifest_url`, verify the manifest signature chain to the
marketplace CA, and then delegate to the existing
`plugin-extension-framework` install flow with its capability
gating unchanged.

#### Scenario: Successful marketplace install

- **WHEN** an Owner installs a CA-verified plugin from the marketplace
- **THEN** the framework installs the plugin under capability gates
        and audit `PluginInstalledFromMarketplace{publisher, id}`
        records the publisher only.

#### Scenario: Unverified publisher refused

- **WHEN** an install is requested for a publisher that fails CA
        verification
- **THEN** the install is refused with
        `PluginMarketplaceError::PublisherUnverified` and no plugin
        is installed.

### Requirement: Ratings and Metadata

The system SHALL surface plugin ratings and metadata from the
catalog (rating, summary, publisher, version) in the marketplace
view; the panel SHALL NOT trust catalog-provided executable content
beyond what the framework's capability model permits.

#### Scenario: Metadata displayed

- **WHEN** an Owner opens a marketplace plugin detail
- **THEN** rating, publisher, and summary are shown; no plugin code
        runs from the metadata fetch alone.

