## ADDED Requirements

### Requirement: Trusted Software Catalog and Discovery
The system SHALL expose an Owner-readable catalog of typed system components and web applications with categories, descriptions, supported versions/platforms, dependencies, licenses, provenance, and lifecycle state. The catalog MUST use an embedded trust root, accept remote manifests only after signature/schema/expiry validation, and classify detected host software without silently taking ownership.

#### Scenario: Catalog signature is invalid

- **WHEN** a remote catalog has an invalid signature, unsupported schema, or expired validity window
- **THEN** the system rejects it, retains the last-known-good catalog, records a redacted audit event, and executes no recipe content

#### Scenario: Existing Nginx was installed externally

- **WHEN** discovery finds a compatible Nginx package not installed by OpenPanel
- **THEN** the catalog reports it as externally managed and neither changes nor removes it without an explicit adoption transaction

### Requirement: System Component Planning and Lifecycle
Owners SHALL preview, install, adopt, update, configure, and uninstall allowlisted system components, initially Nginx, supported PHP-FPM versions/extensions, MySQL or MariaDB, and Redis. Every mutation MUST execute a fresh digest-bound plan through a typed package-manager adapter and SHALL report dependencies, conflicts, downloads, disk impact, affected services, configuration ownership, and rollback capability before confirmation.

#### Scenario: Install PHP version and extensions

- **WHEN** an Owner confirms a valid plan for PHP 8.x with selected allowlisted extensions
- **THEN** the system installs fixed packages, validates PHP-FPM, registers its service, makes the version assignable to sites, and records a secret-free completed job

#### Scenario: Plan changed after preview

- **WHEN** package state or the catalog changes after a preview token was issued
- **THEN** execution rejects the stale token and requires a new preview rather than applying a different transaction

#### Scenario: Dependency remains in use

- **WHEN** an Owner previews removal of Nginx, PHP, MySQL, or Redis while a managed capability still depends on it
- **THEN** the plan reports exact aggregate dependency counts and blocks removal unless a supported migration removes those dependencies

### Requirement: Safe Package Execution and Recovery
The system SHALL serialize host package transactions, use fixed executable paths and arguments, respect native package-manager locks, bound and redact output, snapshot OpenPanel-owned configuration, validate services after changes, and roll back when the recipe declares rollback support. Interrupted jobs SHALL be reconciled on startup and MUST never be reported as successful without post-install validation.

#### Scenario: Package validation fails

- **WHEN** package installation finishes but the component's configuration or readiness validation fails
- **THEN** the job enters rollback, restores owned configuration and package state where supported, leaves the last-known-good services active, and exposes a redacted failure

#### Scenario: Concurrent package request

- **WHEN** another host package job is already active
- **THEN** a new mutation is queued or rejected as busy and no second package-manager process starts

#### Scenario: Panel restarts during installation

- **WHEN** OpenPanel starts and finds a non-terminal job without a live owned process
- **THEN** it marks the job interrupted and offers only recipe-supported retry or rollback actions

### Requirement: One-click Web Application Deployment
Owners SHALL deploy supported pinned releases of WordPress and Drupal from the Software Center by selecting a domain, site settings, PHP version, database options, locale, and optional DNS, TLS, and backup integration. Deployment MUST verify official artifacts, reject unsafe archive entries, use least-privilege credentials, validate application health, and unwind only job-created resources on failure.

#### Scenario: Deploy WordPress successfully

- **WHEN** an Owner confirms a compatible WordPress plan for an unused domain
- **THEN** the system creates the site and database, verifies and extracts the pinned artifact, writes secret-safe configuration, validates the installation, optionally provisions DNS/TLS/backup hooks, and returns generated administrator credentials once

#### Scenario: Drupal requires unavailable PHP extension

- **WHEN** a Drupal release requires a PHP extension not present in the selected runtime
- **THEN** preview reports the missing dependency and offers an explicit combined plan rather than beginning a partial deployment

#### Scenario: Archive contains traversal entry

- **WHEN** an application artifact contains an absolute path, parent traversal, escaping symlink, excessive file count, or expanded-size violation
- **THEN** extraction fails before writing outside staging and all resources created by the deployment are rolled back

### Requirement: Software Jobs, Surfaces, and Observability
REST, CLI, and Owner-only `/software` web surfaces SHALL provide catalog search, installed state, previews, job progress/history, confirmation, cancellation, retry/rollback, component lifecycle, and WordPress/Drupal deployment. Browser mutations MUST enforce CSRF. Responses, logs, audits, and diagnostics MUST exclude repository credentials, application secrets, database passwords, environment secrets, and unbounded package output.

#### Scenario: Non-owner attempts installation

- **WHEN** an Admin or User requests a component or application mutation through any surface
- **THEN** the system returns forbidden before catalog downloads, package commands, filesystem writes, or resource creation

#### Scenario: Owner watches installation progress

- **WHEN** an Owner opens an active job
- **THEN** the panel shows bounded phase-level progress and redacted diagnostics without exposing commands or secrets

#### Scenario: Cancel during unsafe phase

- **WHEN** an Owner requests cancellation while the package manager is in a non-interruptible phase
- **THEN** the system records cancellation as pending and transitions only at the next recipe-declared safe checkpoint
