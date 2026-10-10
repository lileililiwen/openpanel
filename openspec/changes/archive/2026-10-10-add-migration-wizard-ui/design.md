# Design: add-migration-wizard-ui

## Explore & Reuse (repo-map evidence, 2026-10-10)

`scripts/repo-map.sh` plus targeted reads confirm every behavior
the wizard needs already exists; the change adds a web adapter
only:

- `crates/openpanel-app/src/migration_importers/service.rs` —
  `MigrationService::preview`, `preview_with_policy`, `run`,
  `run_with_verifiers`, `rollback`. The wizard calls these
  directly; no new service methods.
- `crates/openpanel-app/src/migration_importers/verifiers/mod.rs` —
  `PostImportVerifier` plus `SitesPostImportVerifier`,
  `MailPostImportVerifier`, `DnsPostImportVerifier`,
  `SslPostImportVerifier`, `FailingVerifier`. The run step
  passes the four real verifiers; no new verifier.
- Domain `ConflictPolicy` (`Block` | `Replace` | `Skip` |
  `RequireExplicit`) and `MigrationRunStatus::PartiallyCompleted`
  — the confirm page renders the policy choice and the result
  page renders the partial state with its evidence; no new
  variants.
- `crates/openpanel-app/src/host_ops/cutover.rs` —
  `CutoverService` with `DnsCutover` / `MailCutover` /
  `SslCutover` ports (preflight + audit + rollback TTL). Each
  cutover button maps 1:1 to one port call.
- `crates/openpanel-web/src/layout.rs` — `Shell` and
  `CapabilitySet::shipped()`; `nav_model.rs` (`NAV_SECTIONS`)
  and `capability_registry.rs` (`REGISTRY`, `role_may_see`,
  `template_matches`) for the `/migrate` entry.
- `crates/openpanel-web/src/router.rs` — explicit route
  mounting pattern; no `/migrate` route exists today, so the
  change adds `GET /migrate`, `POST /migrate/upload`,
  `POST /migrate/preview`, `POST /migrate/confirm`,
  `POST /migrate/run`, `POST /migrate/cutover/{dns,mail,ssl}`.
- `crates/openpanel-web/src/host_fleet.rs:147` renders a dead
  `/hosts/{id}/terminal` link; noted in Non-goals, untouched.

New code is limited to one web module
(`crates/openpanel-web/src/migrate.rs`): maud templates for the
five wizard steps, multipart upload parsing into the existing
driver `Source` bytes, and handler glue. No new domain types, no
new app service, no new audit actions (existing
`MigrationPreviewed` / `MigrationRunCommitted` /
`MigrationRunRolledBack` events fire through the reused
services).

## DDD layering

- **Domain** (`openpanel-domain`): untouched. `ConflictPolicy`,
  `MigrationPlan`, `MigrationRunStatus`, `HostCapability` stay
  I/O-free; the wizard introduces zero domain code.
- **App** (`openpanel-app`): reused, not extended.
  `MigrationService` and `CutoverService` are called as-is; the
  change adds no use case, no repository, no migration.
- **Adapters** (`openpanel-web`): the only layer that changes.
  `migrate.rs` handlers resolve `WebUser`, validate CSRF via
  `ValidateCsrf`, call the app services, and render maud
  fragments with the shared `form` / `table` / `btn` /
  `op-empty-state` / `op-error-state` tokens (no new CSS).
  Role gating is defense in depth: the nav entry hides below
  `Owner`, and handlers deny direct access with 403.

## Page flow

1. `GET /migrate` — upload form (`form`, file input +
   driver-hint select: auto / Baota / cPanel pkgacct).
2. `POST /migrate/upload` + sniff — stores bundle bytes
   server-side keyed by a one-time upload id; renders format
   result or `MalformedSource` inline error.
3. `POST /migrate/preview` — `preview_with_policy`; renders
   resources table, warnings list, conflicts list.
4. `POST /migrate/confirm` — `ConflictPolicy` radio
   (Block/Replace/Skip/RequireExplicit); `Block` and
   `RequireExplicit` refuse commit while conflicts persist.
5. `POST /migrate/run` — `run_with_verifiers`; renders
   committed set, per-verifier evidence, and the
   `PartiallyCompleted` banner (which refuses cutover) when any
   verifier fails.
6. `POST /migrate/cutover/{dns,mail,ssl}` — separate
   authorized actions, each with preflight result, audit entry,
   and rollback/TTL note; refused while the run is partial.

Upload size is capped by the existing request-body limit;
bundles never persist beyond the wizard session, and error
pages never echo bundle bytes or secrets.

## Roles and discoverability

Registry entry `key: "migrate"`, route `/migrate`, capability
`migration`, `min_role: Owner`, scope Global, Operations group.
Direct `User`-role access returns 403 without leaking plan
content.

---

*This design AWAITS human principal approval per Agents.md §8
item 10 — do NOT implement until approved.*
