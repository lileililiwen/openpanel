# Add OS Update Management

## Why

There is **no OS / package security-update management UI** in
OpenPanel. Servers drift on unapplied CVEs because operators must drop
to the shell and run `apt` by hand, with no visibility into which
updates are security-related, whether unattended upgrades are enabled,
or whether a reboot is pending after a kernel update. This change adds
an `os-updates` bounded context exposed to administrators.

## What Changes

- New bounded context `os-updates` carrying the `UpdatePolicy`
  aggregate and `UpdateLister`, `UpdateApplier`, `RebootState`.
- New endpoints: `GET /admin/os/updates`,
  `POST /admin/os/updates/apply`, `PUT /admin/os/updates/policy`.
- List available `apt` updates classified as security vs other; apply
  security updates; configure `unattended-upgrades`; expose kernel /
  update history with a reboot-required flag.

## Capabilities

### New Capabilities

- `os-updates`: list and classify pending apt updates, apply security
  updates, configure unattended-upgrades, and report update history
  with a reboot-required flag.

## Impact

- Domain: `PackageUpdate`, `UpdatePolicy`, `RebootState`, `UpdateKind`.
- App: `UpdateLister`, `UpdateApplier`, `UnattendedConfig`.
- API/CLI/web: `/admin/os/updates*`, web Admin → OS Updates panel.
- Security: update apply is Admin-gated and audited; apt invocations
  are allow-listed (no free-form shell input from the UI).
- Coupling: extends `software-center` package surface; complements the
  host hardening view from `host-security` (archived).
