# site-staging Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-site-staging. Update Purpose after archive.
## Requirements
### Requirement: StagingSlot Lifecycle

The system SHALL let an authorised caller create, list, and
delete a `StagingSlot` per site. A slot MUST live under the
site's `document_root` parent (`/var/www/<domain>/staging/`)
and MUST use a database name `<owner>_<site>_staging`. PHP
runtime MIRRORS production by default; on promotion, prod
gains the staging runtime.

#### Scenario: Create staging under a site

- **WHEN** an Owner posts `POST /sites/{s1}/staging/create`
- **THEN** a `StagingSlot` row exists, the document_root is
        `/var/www/<domain>/staging/public_html`, and the
        staging DB is created.

#### Scenario: Cannot escape chroot

- **WHEN** the create request is made with a custom
        `document_root` outside the site chroot
- **THEN** the request is rejected with `SiteStagingError::OutsideChroot`.

### Requirement: Sync Modes

The system SHALL support `OnDemand`, `OnPromote`, and
`Scheduled` sync policies. On a sync, files are rsynced and
the prod DB is dumped into the staging DB; PII in user tables
is anonymised by default and the anonymisation policy is
auditable.

#### Scenario: Snapshot

- **WHEN** an Owner runs `sync(mode=snapshot)` on site `s1`
- **THEN** staging docroot mirrors prod; staging DB is a
        freshly-dumped copy; `staging_snapshots` row has a
        monotonically increasing `snapshot_id`.

#### Scenario: PII anonymised

- **WHEN** the anonymisation policy is "standard" (default)
- **THEN** user table emails are replaced with
        `<id>@staging.local`; audit `StagingPiiAnonymised`
        records row counts only.

### Requirement: Atomic Promote

`POST /sites/{id}/staging/promote` SHALL swap the
document roots under a single rename chain with nginx reload
in between. The promote MUST require a fresh `confirmed_at`
within 60 seconds and MUST roll back if any step fails.

#### Scenario: Successful promote

- **WHEN** an Owner promotes within the snapshot window
- **THEN** the new docroot is at `/var/www/<domain>/public_html`;
        nginx served the new content after reload; audit
        `StagingPromoted` records the snapshot id.

#### Scenario: nginx reload fails

- **WHEN** nginx reload fails after the rename
- **THEN** the docroot rename chain is reversed; nginx reload
        reverts; audit `StagingPromotionRolledBack{redacted}`.

### Requirement: Lock and Concurrent Writes

During a sync or promote the staging slot MUST block writes;
the lock is per slot, with a 5-minute maximum hold; the lock
auto-releases on writer error or process exit.

#### Scenario: Concurrent sync refused

- **WHEN** a sync is in progress and a second sync is requested
- **THEN** the second request returns `409 sync_in_flight`.

#### Scenario: Stale lock cleared

- **WHEN** a slot lock is older than 5 minutes
- **THEN** the next request acquires the lock and audit
        `StagingSlotLockReclaimed` is recorded.

### Requirement: Staging Destruction

`DELETE /sites/{id}/staging` SHALL remove the staging
document_root, drop the staging DB (after a typed destructive
confirmation), and remove the `staging_slots` row. A site
that has been promoted against is treated as having no
staging state by default; destruction is idempotent.

#### Scenario: Destruction with confirmation

- **WHEN** an Owner submits `DELETE /sites/{s1}/staging` with
        `confirmed_at` and `drop_db=true`
- **THEN** files are removed, the DB is dropped, and the
        staging slot is gone.

