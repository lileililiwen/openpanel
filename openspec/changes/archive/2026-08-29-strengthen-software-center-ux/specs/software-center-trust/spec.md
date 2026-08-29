## ADDED Requirements

### Requirement: Catalog trust is visible

Every software item MUST show source, publisher/signature or digest state, compatibility, permissions, dependencies, conflicts, and installed state.

#### Scenario: Artifact has placeholder digest

- **WHEN** a catalog item has a placeholder digest
- **THEN** installation remains blocked and the exact safe recovery action is shown

### Requirement: Preview precedes mutation

Install, update, remove, and application deployment MUST provide a read-only plan before execution.

#### Scenario: Owner reviews install

- **WHEN** an owner requests installation
- **THEN** the UI shows impact, dependencies, conflicts, and required confirmation before mutation

### Requirement: Transactions are recoverable

Long-running software actions MUST expose progress, cancellation where supported, safe failure details, retry, and rollback where supported.

#### Scenario: Install fails

- **WHEN** an installation fails
- **THEN** the UI shows a correlation ID and available retry/rollback actions without secrets
