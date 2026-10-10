# migration-importers Specification

## ADDED Requirements

### Requirement: Migration Wizard Surfaces The Existing Preview-Run Lifecycle

The `/migrate` web wizard MUST surface the existing
`MigrationService` lifecycle without adding new import logic:
upload plus sniff, `preview_with_policy` rendering of resources,
warnings, and conflicts, operator-confirmed `ConflictPolicy`,
`run_with_verifiers`, and `PartiallyCompleted` handling. The
wizard MUST NOT execute source scripts and MUST NOT introduce
new conflict semantics, verifier logic, or audit actions.

#### Scenario: Wizard previews without mutating

- **WHEN** an operator uploads a Baota `/www/backup` bundle or a
  cPanel `pkgacct` bundle and requests a preview
- **THEN** the wizard sniffs the format, lists the recognised
  resources, warnings, and conflicts, and commits nothing.

#### Scenario: Block policy refuses a conflicting commit

- **WHEN** the confirmed `ConflictPolicy` is `Block` (or
  `RequireExplicit` without explicit confirmation) and the plan
  carries conflicts
- **THEN** the wizard refuses the commit and reports the
  blocking conflicts with no state change.

#### Scenario: Partial verification surfaces without cutover

- **WHEN** `run_with_verifiers` commits but a post-import
  verifier reports failure
- **THEN** the wizard renders the run as `PartiallyCompleted`
  with actionable evidence and disables DNS/mail/SSL cutover.

### Requirement: Wizard Cutover Actions Stay Separate And Authorized

DNS, mail, and SSL cutover from the wizard MUST be separate
authorized actions delegating to the existing `CutoverService`
ports, each with preflight, audit, and a declared rollback or
recovery path. Cutover MUST be refused while the run is
`PartiallyCompleted` or while its preflight fails.

#### Scenario: Cutover refused on partial run

- **WHEN** an operator requests DNS cutover for a
  `PartiallyCompleted` run
- **THEN** cutover is refused and the imported source remains
  isolated.
