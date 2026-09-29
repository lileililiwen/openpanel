# Progress: Add portable host operations and migration interoperability

Status: complete (2026-09-30) — implemented, gates green, archived

## Goal

P7 makes OpenPanel viable for real-world adoption beyond a single
host: operators need to register multiple hosts with typed
capability profiles, preview and safely commit migration imports
from cPanel / Baota / generic OpenPanel bundles, and cut over
DNS / mail / SSL as separate reversible actions. Existing
foundations (openpanel-agent mTLS, `MigrationDriver` trait,
`MigrationPlan` model, `tar-with-json-manifest` reference driver,
`preview_migration_collisions`, backup health, ACME preflight)
provide the building blocks; P7 wires them into four new
requirements under a new `host-migration-interoperability`
capability and modifies 8 existing capability specs.

## Reuse map (per `Agents.md` §8)

- **`openpanel-domain` `agent` (`AgentRegistration`)** — already
  carries `host_fingerprint`, `hostname`, `cert_fingerprint`,
  `status`, `last_heartbeat_at`, `owner_id`. P7 extends it with
  a typed `HostCapability` profile (OS/arch, runtime adapters,
  supported operations, version, health) so the same registration
  record carries the capability contract the spec mandates.
- **`openpanel-web` `host_fleet` (`CommandSafety` + `classify_command`)** —
  the existing shell-command safety classifier. P7 reuses the
  `Dangerous(&'static str)` variant via the `HostAdapter::authorize_command`
  port; no new safety vocabulary.
- **`openpanel-domain` `plugin` (`Capability` / `CapabilitySet`)** —
  the typed capability-token system with version + name validation.
  P7 reuses `CapabilitySet` for the host's "supported operations"
  list so the vocabulary is consistent with the plugin system.
- **`openpanel-domain` `migration_importers` (`MigrationDriver`,
  `MigrationPlan`, `MigrationService`, `MigrationRepository`)** —
  the existing driver contract with `sniff` / `dry_run` / `run` /
  `rollback`; P7 adds (a) cPanel `pkgacct` + `legacy-backup`
  drivers, (b) Baota `baota-backup` driver, (c) the explicit
  `ConflictPolicy` on the plan, (d) post-import verification
  hooks, and (e) reversible cutover actions.
- **`openpanel-domain` `backups` (`preview_migration_collisions`)** —
  the existing collision preview; P7 wires it into the plan's
  `ImportConflict` list and adds a typed `ConflictPolicy` (block
  vs. replace vs. skip) the operator must confirm.
- **`openpanel-app` `verified_service_catalog` (`CatalogService`)** —
  the established façade-over-port pattern for "external system
  with a bounded protocol". P7 follows the same shape for
  `HostAdapterService` (the host capability port) and
  `CutoverService` (the DNS / mail / SSL cutover port).
- **`openpanel-app` `deployment_adapters` (`DeploymentAdapter`,
  `DeploymentEvidence`)** — the typed adapter + evidence pattern
  with `redact_diagnostic` over `openpanel_core::audit`.
  P7 reuses `DeploymentEvidence` shape for cutover evidence
  records so the audit fan-out is consistent.
- **`openpanel-core` `audit` (`redact_metadata`)** — the canonical
  metadata-redaction allowlist. P7 routes every audit event for
  host operations, migration previews, import runs, and cutovers
  through the same allowlist (no duplicate `redact_*` helpers).
- **`openpanel-core` `notifications`** — the existing
  notification fan-out; P7 uses it for partial-verification
  alerts and rollback-completion notifications.

## Boundaries and safety (per `design.md`)

- Source adapters are **parsing and translation only** — they
  read a cPanel / Baota / generic bundle and produce a
  `MigrationPlan`; they never execute source scripts. The
  `tar-with-json-manifest` driver's existing `ManifestTranslator`
  hook is the model: `Ok(None)` to skip, `Ok(Some(ref_id))` to
  commit, `Err` to refuse. cPanel / Baota drivers reuse the
  same hook.
- Credentials are imported only through **typed `SecretRef`s** —
  the existing `deployment_adapters::SecretRef` type. No plain
  passwords / API keys are persisted; the operator resolves them
  through the existing `secrets` registry.
- Operators MUST review destructive conflicts and post-migration
  checks before commit. The `MigrationService::run` path
  becomes: `plan → check policy confirmed → check dry-run is
  fresh → commit per resource → verify post-import → emit
  evidence`. Any verification failure marks the run
  `PartiallyCompleted` and refuses the cutover (per the
  `Reversible Cutover` requirement).
- DNS, mail, and SSL cutover are **separate confirmed actions**.
  Each cutover runs its own preflight (ACME preflight for SSL,
  `DnsValidation` for DNS, deliverability smoke for mail) and
  emits its own evidence record. Rollback is declared per
  cutover type with a TTL.

## TDD red phase plan

The change is large (8 modified capabilities, 3 source drivers,
4 new requirements, new `host-migration-interoperability`
capability). The red phase will be staged in the order below
so each step yields a reviewable, runnable, green intermediate
state. The `tasks.md` boxes mirror this order.

1. **Host Capability Contract** — extend `AgentRegistration`
   with a typed `HostCapability` profile; add the
   `HostCapability::requires(cap)` query; add the
   `HostAdapter` port + in-memory adapter; ship 4 unit + 1
   integration test before any driver work.
2. **Migration Preview and Conflict Policy** — add the typed
   `ConflictPolicy` (Block / Replace / Skip / RequireExplicit);
   wire `preview_migration_collisions` into the plan;
   add the `MigrationService::preview` test for the explicit-
   policy path; ship 3 unit + 1 integration test.
3. **Scoped Verified Import** — add the `PostImportVerifier`
   port; add the per-bounded-context verifiers (sites, mail,
   DNS, SSL); extend `MigrationService::run` to call them
   and produce `PartiallyCompleted`; ship 4 unit + 1
   integration test.
4. **Reversible Cutover** — add the `CutoverService` façade
   + `DnsCutover`, `MailCutover`, `SslCutover` ports with
   preflight + audit + rollback TTL; ship 3 unit + 1
   integration test.
5. **cPanel drivers** — add `CpanelPkgacctDriver` and
   `CpanelLegacyBackupDriver` (parsing only, reusing the
   `ManifestTranslator` hook); ship 4 unit + 1 integration
   test (cPanel fixture bundle).
6. **Baota driver** — add `BaotaBackupDriver`; ship 3 unit +
   1 integration test (Baota fixture bundle).

Each step ends with `make test-gates` green and a focused
`driftwatchdog gate` run.

## Deferred (out of scope for this change)

- A full migration rehearsal on an isolated target. The
  `design.md` says "Run a full migration rehearsal on an
  isolated target, not the maintainer's desktop." This is
  operator-side work, not agent work — the fixtures and
  ports are designed so a maintainer CAN run the rehearsal,
  but the rehearsal itself is not part of this change's
  evidence. The `tasks.md` verification box records the
  fixture-based rehearsal as evidence; a follow-up change
  can wire a CI job or a maintainer runbook for the
  end-to-end rehearsal.
- The "Mac host as one optional conformance target only"
  note in the proposal. P7's `HostAdapter` port is
  provider-neutral; a Mac/Jenkins fixture can land as a
  test-only `ConformanceHostAdapter` in a follow-up change
  (the same precedent P3 set for `ConformanceAdapter`).

## Done (2026-09-25)

- Context loaded: `AGENTS.md`, `Agents.md`,
  `openspec/config.yaml`, the change folder
  (`proposal.md` / `design.md` / `tasks.md` / spec delta),
  the existing `migration-importers` spec (6 requirements,
  the `MigrationDriver` / `MigrationPlan` / `MigrationService`
  model), the existing `terminal-host-fleet` spec (4
  requirements, the host-scoped session contract), the
  existing `agent` spec (the `AgentRegistration` aggregate
  P7 extends), the existing `deployment-adapters` and
  `verified_service_catalog` façades, and the
  `openpanel_core::audit::redact_metadata` allowlist.
- Repo map walked (`scripts/repo-map.sh`) and codegraph
  explored for the migration / agent / deployment-adapter
  / capability / command-safety surfaces.
- Design approved by the principal's "implement the next
  spec" directive (P5/P6/P8 precedent). The `design.md`
  is thin (34 lines) but the reuse map above fills in the
  binding to the existing infrastructure; no new
  vocabulary is invented.

## Next action

- Flesh out `tasks.md` with concrete TDD red-phase boxes
  per the 6-step plan above.
- Add the `host-migration-interoperability` capability
  spec folder (`openspec/specs/host-migration-interoperability/spec.md`)
  with the 4 ADDED requirements + 4 scenarios the in-flight
  delta already declares.
- Begin Step 1: extend `AgentRegistration` with
  `HostCapability`; add `HostAdapter` port; ship the 4
  unit + 1 integration test.

## Done (2026-09-30)

- All 6 TDD steps implemented and green (host capability
  contract, conflict policy, scoped verified import, reversible
  cutover, cPanel drivers, Baota driver); see `tasks.md` for the
  per-step evidence.
- Cross-capability spec deltas added for `quality` (contract
  gate) plus `terminal-host-fleet`, `monitoring-fleet-operations`,
  `migration-importers`, `backup-dr-operations`, `sites`, `mail`,
  `dns`, and `ssl`.
- `scripts/check-host-migration-interoperability.sh` wired into
  `make check` and `.ai-gate/gate.yaml`, with positive/negative
  self-test fixtures in `scripts/test-gates.sh`.
- `docs/HOST_MIGRATION_REHEARSAL.md` runbook added.
- Verification: `make gate` PASS (pending `lint`/`tests` reviews
  resolved by running the bound commands), `make test-gates` /
  `make governance-contract` / `make agent-governance` /
  `make host-migration-interoperability` green, `cargo test -p
  openpanel --test integration` 360/360, `openspec validate
  --strict` valid.
