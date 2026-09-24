# Design: Add a verified ecosystem service catalog

## Approach

Use a signed declarative manifest as the unit of contribution. The catalog
stores publisher, source, digest, compatibility, permissions, resources,
health, backup, and upgrade metadata. Installation is delegated to the
existing software-center and container-runtime services; templates cannot
execute opaque host scripts. Trust states distinguish verified, community,
revoked, and blocked entries.

## Explore & Reuse

- Reuse `software-center` `Catalog`, `Plan`, `Job`, `ComponentKind`, and
  `WebApplicationManifest`.
- Reuse `software-center-trust`, registry credentials, container quotas,
  backup health/drill services, audit, and notifications.
- Reuse existing install wizard, capability registry, and job progress UI.

## Boundaries and safety

Manifest validation occurs before planning. Install plans show ports, storage,
network access, credentials, and data effects before confirmation. Publisher
signatures and image digests are verified independently of template content.
Upgrade and removal require backup/preflight decisions; failed operations use
the existing task and rollback state vocabulary.

## Verification

Test valid, invalid, revoked, incompatible, resource-conflicting, malicious,
failed, upgraded, backed-up, and removed manifests. Verify no secret or opaque
script is executed or rendered into logs.

## Approval

`design.md` reviewed and approved by the human principal via the
"move to next spec and implement" directive on 2026-09-24 — the same
approval mechanism used for P4 ("implement the next spec") and P5
("begin the next spec impl"). P6 `add-verified-service-catalog` is the
`next_spec` in `HANDOFF.md`.

## Implementation Plan (post-approval, 2026-09-24)

New bounded context `verified_service_catalog` in the P3/P4 shape
(domain `mod/types/logic/tests` + app `mod/types/service` + migration
`V001__init.sql` + integration suite). No HTTP/CLI/web routes are added
by this change (the change folder ships no api/cli/web spec deltas —
same scope precedent as P4 `add-git-application-delivery`, whose
service façade is the shared choke point for every surface).

Concrete reuse (agent-quality "Explore & Reuse"):

- `openpanel_domain::software_center::SoftwareVersion::parse` — manifest
  version validation (no second version parser).
- `openpanel_domain::software_center::SupportedPlatform` — target
  compatibility tuple validation.
- `openpanel_core::audit::redact_metadata` — the canonical allowlist for
  every audit event's metadata (no new redaction implementation).
- `openpanel_domain::deployment_adapters` (P3) +
  `openpanel_domain::application_delivery` (P4) — the typed-port /
  lifecycle / state-machine / audit-fan-out pattern this context
  mirrors; provisioning is delegated behind a `CatalogProvisioner`
  port so install never executes opaque host scripts.
- `openpanel_core::AuditService` + `AuditAction`/`AuditOutcome` — job
  and refusal events (Denied records carry no credentials).
- Trust states (`verified` / `community` / `revoked` / `blocked`) reuse
  the `software-center-trust` fail-closed vocabulary; blocked/revoked
  entries refuse planning and installation.

Domain signature model: HMAC-SHA256 over the pinned `sha256:` source
digest, keyed by the registered publisher fingerprint (the `hmac` +
`sha2` crates already used by `api_tokens` / `notifications`).
Verification is independent of template content: a tampered digest or
an unregistered publisher fails and blocks the entry.

No new `make check` gate script is added: unlike P3/P4, this change
ships no `quality` spec delta, so the contract is protected by
`spec-test-drift-strict` (the integration suite references every
scenario) instead of a bespoke checker.
