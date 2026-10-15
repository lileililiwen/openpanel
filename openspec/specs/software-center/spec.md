# software-center Specification

## Purpose

The software-center bounded context covers the curated catalog,
recipes, plans, jobs, and the ensure pipeline. After this
refinement, it also owns the `ComponentKind` classification
(`System` vs `Application`) and the typed `WebApplicationManifest`
that the per-site installer consumes.
## Requirements
### Requirement: Component Classification

The software-center bounded context SHALL model a `ComponentKind` enum (`System | Application`). Every catalog entry carries a `kind` and the install dispatcher refuses to install an entry through the wrong adapter.

#### Scenario: Install kind mismatch

- **WHEN** the entry's `kind` is `System` and the adapter's `kind` is `Application`
- **THEN** `check_install_kind` returns `SoftwareCenterRefineError::InstallKindMismatch`.

#### Scenario: Install kind match

- **WHEN** the entry's `kind` matches the adapter's `kind`
- **THEN** `check_install_kind` returns `Ok(())`.

### Requirement: Web Application Manifest

The software-center bounded context SHALL model a `WebApplicationManifest { id, name, version, target_site_type, requires_db, requires_php, requires_storage_bytes, install_signature }`. The constructor rejects empty id, name, or version.

#### Scenario: Constructor rejects empty inputs

- **WHEN** `WebApplicationManifest::new("", "WordPress", "6.0", ...)` is called
- **THEN** the constructor returns `SoftwareCenterRefineError::InvalidManifest`.

#### Scenario: Manifest accepts valid payload

- **WHEN** all fields are present and non-empty
- **THEN** the manifest is constructed and the accessors are populated.

### Requirement: Target Site Type

The software-center bounded context SHALL model a `TargetSiteType` enum (`SingleSite | MultiTenant`). The `WebApplicationManifest` carries one target site type.

#### Scenario: SingleSite target

- **WHEN** the manifest declares `TargetSiteType::SingleSite`
- **THEN** the application layer installs one instance per site.

### Requirement: Behaviour Parity

The refinement SHALL introduce the new types without changing the existing `Catalog`, `Plan`, and `Job` lifecycle. The follow-on `add-web-application-installer` change wires the installer around the typed manifest.

#### Scenario: Existing behaviour unchanged

- **WHEN** the refined bounded context is exercised through its public API
- **THEN** behaviour outside the newly added surface is identical to the pre-refinement behaviour.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `WebApplicationInstalled`, `WebApplicationUninstalled`, and `WebApplicationInstallKindMismatch` audit events. The bounded context as archived today owns the typed model and the validation rules.

#### Scenario: Install is audited

- **WHEN** a web application install completes
- **THEN** an audit `WebApplicationInstalled` event records the
        application kind and site id without credential material.

### Requirement: Version-Targeted Installs

The Software Center SHALL accept an optional `version` on preview and
install operations. A missing version resolves to the entry's
`latest_version`; an unknown version (not in the entry's `versions`)
is rejected with a stable error.

#### Scenario: Explicit version install

- **WHEN** an authorized operator previews install of an entry with
  `version` set to a listed `VersionSpec`
- **THEN** the preview resolves packages from that version's spec and
  the install records a component row keyed by (component id, version).

#### Scenario: Unknown version rejected

- **WHEN** an operator requests install with a version absent from the
  entry's `versions`
- **THEN** the service returns `SoftwareCenterError::UnknownVersion`
  and no component row is written.

#### Scenario: Default version unchanged behaviour

- **WHEN** an operator installs without specifying a version
- **THEN** the latest version is installed, matching pre-change
  behaviour.

### Requirement: Co-Installable Versions

The software center SHALL allow multiple installed versions of the same
component to coexist; inventory and update badges are computed per
(component id, version) row against the catalog `latest_version`.

#### Scenario: Two versions installed

- **WHEN** versions `1.0` and `2.0` of a component are both installed
- **THEN** inventory lists both rows and the component is reported
  installed.

#### Scenario: Update badge per version

- **WHEN** an installed version differs from the catalog
  `latest_version` while another installed version equals it
- **THEN** the update-needed set includes only the outdated version row.

#### Scenario: Uninstall one version keeps the other

- **WHEN** an operator uninstalls a specific version while another
  version of the same component remains installed
- **THEN** only that (id, version) row is removed and audit records the
  version.
