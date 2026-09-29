# Design: Add portable host operations and migration interoperability

## Approach

Represent host capabilities and migration as typed plans. A host adapter
reports capabilities and executes only authorized operations. A migration
driver sniffs a source, produces a bounded plan with warnings/conflicts, then
commits through existing site, mail, DNS, SSL, backup, and audit services.
Verification runs before any DNS or traffic cutover and rollback remains
available for the declared window.

## Explore & Reuse

- Reuse `openpanel-agent` mTLS registration, fleet health, command safety, and
  audit redaction.
- Reuse `migration-importers` `MigrationDriver`, `MigrationPlan`, preview/run/
  rollback lifecycle, and manifest driver.
- Reuse backup health/drills, site staging, mail authorization, DNS validation,
  ACME preflight, and existing notifications.

## Boundaries and safety

Source adapters are parsing and translation boundaries; they do not execute
source scripts. Unsupported source features become explicit warnings or hard
conflicts. Credentials are imported only through typed secret references.
Operators must review destructive conflicts and post-migration checks before
commit. DNS, mail, and SSL cutover are separate confirmed actions.

## Verification

Use fixture bundles for supported source formats and test capability mismatch,
unsafe paths, collisions, partial failure, rollback, DNS/TLS verification,
mail checks, and secret-safe evidence. Run a full migration rehearsal on an
isolated target, not the maintainer's desktop.
