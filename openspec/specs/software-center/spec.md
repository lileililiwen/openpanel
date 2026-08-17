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

The refinement introduces the new types without changing the existing `Catalog`, `Plan`, and `Job` lifecycle. The follow-on `add-web-application-installer` change wires the installer around the typed manifest.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `WebApplicationInstalled`, `WebApplicationUninstalled`, and `WebApplicationInstallKindMismatch` audit events. The bounded context as archived today owns the typed model and the validation rules.
