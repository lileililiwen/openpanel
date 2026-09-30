# readiness Specification

## Purpose
TBD - created by archiving change post-mvp-readiness. Update Purpose after archive.
## Requirements
### Requirement: Web UI screenshot evidence

The README SHALL embed a committed screenshot set covering the login/shell, the
operations dashboard, the site workspace, the audit activity center and the
software center, each with a caption and a note that captures use synthetic demo
data and contain no secrets or real host data.

#### Scenario: Shipped UI is visible

- **WHEN** a reader opens the README
- **THEN** each listed Web UI surface is shown by an embedded image whose path
  resolves to a committed asset.

#### Scenario: No secrets in captures

- **WHEN** a screenshot is reviewed before commit
- **THEN** it shows only the dev bootstrap's synthetic state and contains no
  secret, token, private key or real host/user data.

### Requirement: Capability catalog linkage

The README SHALL link a capability catalog that enumerates every live capability
from `openspec/specs/` with its spec path, and the catalog SHALL be reproducible
by a command so it cannot drift from the live spec set.

#### Scenario: Capability surface is navigable

- **WHEN** a reader wants to find a capability
- **THEN** the catalog lists it with a working link to its spec, and the README
  links the catalog.

#### Scenario: Catalog cannot drift

- **WHEN** a capability is added, renamed or removed
- **THEN** regenerating the catalog reflects the live spec set, and an
  unregenerated catalog fails the documentation check.

### Requirement: Documentation readiness is enforced

The documentation leg of `make check` SHALL fail when a README screenshot asset
or in-repo relative link does not resolve, or when the capability catalog is
stale, and SHALL pass on the corrected tree; the governance self-test SHALL
carry positive and negative fixtures for the new rule.

#### Scenario: Missing asset or stale catalog fails

- **WHEN** a screenshot is absent or the catalog does not match `openspec/specs/`
- **THEN** `make docs` (via `make check`) exits non-zero and names the artifact.

#### Scenario: Corrected tree passes

- **WHEN** assets, links and the catalog are all current
- **THEN** `make docs`, `make test-gates` and `make check` exit zero with the
  existing checks still green.

### Requirement: README gate description matches the repository

The README's description of the `make check` chain SHALL match `Makefile` and
`AGENTS.md`, so the documented quality gate cannot silently diverge from the
executed one.

#### Scenario: Gate chain is accurate

- **WHEN** the README's gate chain is compared with `Makefile`
- **THEN** every listed step exists and no executed step is omitted.

