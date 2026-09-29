# migration-importers Specification

## Requirements

## ADDED Requirements

### Requirement: Portable Source Adapters and Conflict Policy

The migration driver contract MUST support portable source adapters for cPanel
(`pkgacct` and legacy backup), Baota backup, and generic OpenPanel bundles. Each
adapter MUST be parsing-and-translation only — it MUST NOT execute source
scripts — and MUST reuse the existing `ManifestTranslator` hook: `Ok(None)` to
skip a resource, `Ok(Some(ref_id))` to commit it, and `Err` to refuse the
import. A plan MUST carry a typed `ConflictPolicy` (`Block` | `Replace` |
`Skip` | `RequireExplicit`) that the operator confirms before commit; `Block`
and `RequireExplicit` MUST refuse a commit while conflicts are present, and
`Replace` MUST record a warning per replaced resource. A commit that succeeds
but fails post-import verification MUST be recorded as
`MigrationRunStatus::PartiallyCompleted` and MUST NOT permit automatic cutover.

#### Scenario: Portable source adapter previews and imports

- **WHEN** an operator previews a cPanel `pkgacct`, a cPanel legacy backup, a
  Baota backup, or a generic OpenPanel bundle
- **THEN** the driver sniffs the format, the plan lists the users, sites, and
  databases it recognises, unsupported source features become explicit
  warnings, no source script is executed, and a conflict-free plan commits
  through the existing bounded-context services.

#### Scenario: Partial verification marks the run incomplete

- **WHEN** a scoped import commits but a post-import verifier reports a failure
- **THEN** the run is recorded as `partially_completed` with actionable
  evidence and cutover is refused.
