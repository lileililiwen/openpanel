## Purpose

Implements the install path for typed web applications (such as
WordPress, Ghost, Joomla) over the catalog classification
introduced by `refine-software-center-with-app-classification`.
Installs are previewable, atomic, idempotent, and reversible
within the rollback window.

# web-application-installer Specification

## Requirements

### Requirement: Typed Install Plan

The system SHALL produce an `InstallPlan` for any
`{ app_id, site_id }` input. The plan SHALL contain: `downloads[]`
with `url`, `sha256`, `size`, `signature_url`; `extracts.install_path`;
a `db` sub-document; an overlay list; `warnings[]` and an
SHA-256 `content_hash`. A plan SHALL be valid for at most 5
minutes; an expired plan fails closed with `PlanExpired` on run.

#### Scenario: Plan preview is read-only

- **WHEN** an Owner POSTs `dry_run=true`
- **THEN** the response is the typed plan only; no files are downloaded; no DB rows are created; no subprocess runs.

#### Scenario: Plan expires

- **WHEN** the run request arrives more than 5 minutes after the preview
- **THEN** the run fails with `PlanExpired`; no I/O happens.

### Requirement: Install Run

The system SHALL run an install only after a fresh `confirmed_at`
within ±60 seconds. The run SHALL: verify plan content_hash,
download each artefact, verify sha256, verify signature,
extract under the site root, create a fresh DB (`db_name` =
`{owner_username}_{requested_name}` per the `databases` spec),
write the config overlay, encrypt the secret under the master
key, persist `web_app_installs`, and audit `WebAppInstalled`.

#### Scenario: Successful install

- **WHEN** an Owner runs an install with a fresh plan
- **THEN** the response is `{ install_id, post_install_url, install_path }` and the secret is returned exactly once.

#### Scenario: Tampered artifact

- **WHEN** the sha256 does not match
- **THEN** the run fails; partial files are rolled back; audit `InstallArtifactRejected` records the kind and redacted reason; no DB row persists.

### Requirement: Concurrency and Idempotency

A `(site_id, app_id)` lock SHALL serialise installs. A
concurrent request returns `409` with `install_in_flight`. A
repeated `run` with the same `idempotency_key` within 24 hours
returns the prior result without redoing work; with a fresh
key, the prior state is checked and a re-install is rejected
if `app_id` already exists at the same path.

#### Scenario: Concurrent run rejected

- **WHEN** two clients POST runs against the same `(site_id, app_id)` within 1s
- **THEN** the first wins; the second receives `409 install_in_flight`.

#### Scenario: Replay with same idempotency key

- **WHEN** a client retries a run with the same `idempotency_key` and an unmatched `plan_id`
- **THEN** the response is the original install result; no re-download or re-install happens.

### Requirement: Upgrade Path

The system SHALL preview and execute upgrades via the
`/web-apps/{id}/upgrade` endpoint. The system SHALL refuse an
upgrade whose target manifest's `config_overlay_paths` differ
non-additively from the installed version unless a typed
`confirmation.destructive=true` is supplied within 60 seconds.

#### Scenario: Routine upgrade

- **WHEN** an Owner upgrades from version 1.0 to 1.1 with additive overlay changes only
- **THEN** the upgrade runs without confirmation and audit `WebAppUpgraded{from=1.0, to=1.1}` is recorded.

#### Scenario: Overlay diff

- **WHEN** the overlay paths differ non-additively
- **THEN** the upgrade is rejected with `OverlayDiffRequiresConfirmation{redacted_paths}` until a typed destructive confirmation is supplied.

### Requirement: Uninstall

The system SHALL support uninstalling a web app via
`DELETE /web-apps/{id}` requiring a typed destructive
confirmation within 60s. The installation SHALL refuse to drop
the database unless `drop_db=true` is set; the file tree at
the install path SHALL be removed after audit-only backup to a
configurable `webapp_uninstall_artifact_dir`.

#### Scenario: Standard uninstall

- **WHEN** an Owner uninstalls without `drop_db`
- **THEN** files are archived to `webapp_uninstall_artifact_dir`, the `web_app_installs` row is marked `Removed`, and the database is left intact.

#### Scenario: Drop database

- **WHEN** `drop_db=true`
- **THEN** files are archived, the row is `Removed`, and the database is dropped via the `databases` spec's destroy path with audit `WebAppUninstalledDroppedDb`.

#### Scenario: Missing confirmation

- **WHEN** `DELETE /web-apps/{id}` is called without `confirmed_at`
- **THEN** the response is `400 missing_confirmation` and no state changes.
