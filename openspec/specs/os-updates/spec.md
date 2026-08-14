# os-updates Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-os-update-management. Update Purpose after archive.
## Requirements
### Requirement: List Updates

`GET /admin/os/updates` SHALL list pending `apt` updates, each
classified as `Security` or `Other` by repository origin, along with a
`reboot_required` flag. The listing SHALL be read-only and MUST NOT
mutate system state.

#### Scenario: Security vs other classified

- **WHEN** an Admin fetches `/admin/os/updates` with pending updates
- **THEN** each `PackageUpdate` carries `kind` of `Security` or
        `Other`, and `reboot_required` reflects kernel/update state.

#### Scenario: No updates

- **WHEN** the host is up to date
- **THEN** the updates list is empty and `reboot_required` is `false`.

### Requirement: Apply Security Updates

`POST /admin/os/updates/apply` with `scope="security"` SHALL install
only security-classified package upgrades. The request SHALL resolve to
allow-listed `apt` arguments and SHALL reject any non-allow-listed
scope. Each apply SHALL be audited and recorded in update history.

#### Scenario: Apply security succeeds

- **WHEN** an Admin applies `scope="security"` with pending security
        updates
- **THEN** security packages upgrade, an `UpdateHistory` row is
        written, and an audit `OsUpdatesApplied{scope}` is recorded.

#### Scenario: Non-allow-listed scope rejected

- **WHEN** the body contains an unrecognised scope value
- **THEN** the request is rejected with
        `OsUpdateError::UnsupportedScope` and no packages change.

### Requirement: Configure Update Policy

`PUT /admin/os/updates/policy` SHALL configure `unattended-upgrades`
(security auto-apply, optional auto-reboot, optional other updates) by
writing the unattended-upgrades config. The change SHALL be audited.

#### Scenario: Enable unattended security upgrades

- **WHEN** an Admin sets `unattended_security=true`
- **THEN** the unattended-upgrades config enables security auto-updates
        and an audit `OsUpdatePolicyChanged` is recorded.

### Requirement: Reboot State

The system SHALL report `reboot_required` after any update that touches
the kernel or other reboot-requiring packages, derived from
`/var/run/reboot-required` or equivalent host signal.

#### Scenario: Kernel update sets reboot flag

- **WHEN** an applied update installs a new kernel package
- **THEN** `reboot_required` becomes `true` and the update history row
        records `reboot_required=true`.

