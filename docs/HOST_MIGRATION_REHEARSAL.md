# Host Migration Rehearsal Runbook

Covers `host-migration-interoperability`: rehearsing a host-to-host
migration on an isolated target before touching production. The
fixtures and ports shipped with the change let a maintainer run this
procedure end to end; the rehearsal itself is deliberately **not** part
of the change's CI evidence, so each operator can run it against their
own target.

## What ships in the change

- **Host capability contract** — `HostCapability` (OS/arch, runtime
  adapters, panel version, health) plus the `HostAdapter` port and the
  `HostAdapterService` façade. A capability-guarded operation is refused
  before mutation with a remediation naming the missing capability.
- **Portable source adapters** — `CpanelPkgacctDriver`,
  `CpanelLegacyBackupDriver`, and `BaotaBackupDriver`. All are
  parsing-and-translation only: they sniff the bundle, build a
  `MigrationPlan`, and reuse the existing `ManifestTranslator` hook.
  They never execute source scripts.
- **Conflict policy** — `ConflictPolicy` (`Block` | `Replace` | `Skip` |
  `RequireExplicit`) attached to the plan and confirmed by the operator
  before commit.
- **Post-import verification** — the `PostImportVerifier` port and the
  per-bounded-context verifiers. A failure records the run as
  `partially_completed` and refuses cutover.
- **Reversible cutover** — `CutoverService` with separate DNS, mail, and
  SSL ports, each with `preflight`, `commit`, and `rollback` (TTL).

Fixture bundles live under `tests/fixtures/cpanel/{pkgacct,legacy}/` and
`tests/fixtures/baota/backup/`.

## Prerequisites

- An isolated target host with a clean OpenPanel install and a registered
  agent (mTLS). Do **not** run the rehearsal against production.
- A redacted copy of the source bundle (cPanel `pkgacct`, cPanel legacy
  backup, or Baota backup). Never commit an unredacted bundle.
- DNS, mail, and SSL cutover planned as three separate windows; each is
  its own confirmed action.

## Procedure

1. **Register and inspect the host.**
   Register the isolated target through the existing agent flow, then
   read its capability profile. Confirm the runtime adapters the import
   needs are present; if a required capability is missing the operation
   is refused before mutation with a remediation.

2. **Preview (no mutation).**
   Run the source driver's `dry_run` (or `MigrationService::preview`)
   against the redacted bundle. Confirm the plan's resources, warnings,
   and conflicts match the source. Unsupported source features must
   appear as explicit warnings — never as silent drops.

3. **Choose a conflict policy.**
   - `Block` / `RequireExplicit` refuse commit while conflicts exist.
   - `Replace` records a warning per replaced resource.
   - `Skip` leaves the conflicting target untouched.
   Destructive conflicts MUST be reviewed against a restored copy first
   (see `docs/BACKUP_DR.md`).

4. **Commit the scoped import on the isolated target.**
   Commit only the resources in the confirmed plan through
   `MigrationService::run`. The commit reuses the existing site, mail,
   DNS, SSL, backup, and audit services — no parallel models.

5. **Verify post-import.**
   Run the per-bounded-context verifiers. A failure marks the run
   `partially_completed` with actionable evidence; do not proceed to
   cutover.

6. **Cut over, one action at a time.**
   For each of DNS, mail, and SSL, run its preflight and only then commit:
   - DNS — `DnsValidation` preflight.
   - Mail — deliverability smoke check.
   - SSL — ACME / reachability preflight.
   A failed preflight refuses the cutover and leaves the source isolated.
   Each cutover records its own evidence and a rollback TTL.

7. **Roll back if needed.**
   Within the declared window, roll back the affected cutover and, if the
   import itself is wrong, roll back the committed run. Restore from the
   source bundle as the last resort.

## Evidence to record

- The capability profile (no secrets) and the plan (resources, warnings,
  conflicts, policy).
- The run status (`completed` vs `partially_completed`) and each
  verifier's evidence.
- Per-cutover preflight results, evidence ids, and rollback windows.
- Wall-clock durations, so future rehearsals can tighten the estimate.

## What is out of scope

- The Mac host is an optional conformance target only; a Mac/Jenkins
  fixture can land as a test-only `ConformanceHostAdapter` in a
  follow-up change (mirroring the `ConformanceAdapter` precedent).
- Automating the rehearsal in CI is a follow-up; this runbook is the
  repeatable manual procedure.
