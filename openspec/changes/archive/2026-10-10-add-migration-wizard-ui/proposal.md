# Proposal: add-migration-wizard-ui

## WHY

Operators adopt OpenPanel by leaving Baota or cPanel, but the
migration path is headless: `MigrationService` (`preview`,
`preview_with_policy`, `run_with_verifiers`), the Baota /
cPanel drivers, `ConflictPolicy`, `PostImportVerifier`, and
`CutoverService` all exist with no browser surface. An operator
must assemble uploads, conflict decisions, verification results,
and DNS/mail/SSL cutover by hand, which invites destructive
mistakes (wrong conflict policy, unverified cutover). The web
router (`crates/openpanel-web/src/router.rs`) mounts no
`/migrate` route, and the capability registry
(`capability_registry.rs`) has no migration entry, so the
workflow is undiscoverable from the shell.

## WHAT

A web wizard at `/migrate` that walks upload → sniff → preview
plan (resources, warnings, conflicts) → `ConflictPolicy` confirm
(Block / Replace / Skip / RequireExplicit) →
`run_with_verifiers` → `PartiallyCompleted` handling, with DNS /
mail / SSL cutover exposed as separate authorized actions backed
by `CutoverService`. Every step reuses the existing app-layer
services; the wizard adds no new import logic and executes no
source scripts (parsing only, per `migration-importers`).

## Capabilities

- **New:** `migration-wizard-ui` — the `/migrate` wizard pages
  and cutover actions (delta in this change).
- **Modified:** `migration-importers` — wizard-facing
  surfacing rules for the existing preview/run lifecycle
  (delta in this change).
- **Modified:** `capability-navigation` — `/migrate` registry
  entry + nav/route agreement (delta in this change).
- Reused unchanged: `host-migration-interoperability`,
  `web-ui` (shell, CSRF, styling tokens), identity/auth.

## Non-goals

- No new auth model; the wizard uses the existing session,
  role gating (`Owner`/`Admin`), and per-session CSRF.
- No execution of source scripts; drivers stay
  parsing-and-translation only.
- No new conflict semantics, verifier logic, or cutover
  semantics — the UI surfaces what the services decide.
- No fix for the dead `/hosts/{id}/terminal` link in
  `host_fleet.rs:147`; recorded as out of scope.
- No bulk-file or backup-wizard endpoints; those belong to
  their own changes.
