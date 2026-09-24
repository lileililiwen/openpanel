# release-evidence Specification

## Purpose

Release evidence is portable, security-reviewed, and sufficient for a
maintainer or downstream operator to verify a published build.

## ADDED Requirements

### Requirement: Audited Dependency Baseline

The release gate MUST fail when the locked dependency graph contains an
unapproved actionable security advisory, and MUST identify the affected crate,
version, advisory, and remediation boundary.

#### Scenario: Actionable advisory

- **WHEN** the lockfile contains a crate version with an actionable advisory
- **THEN** the release gate fails and names the crate, version, advisory, and
  required upgrade or reviewed exception.

#### Scenario: Allowed maintenance warning

- **WHEN** the audit contains only an explicitly allowed unmaintained warning
- **THEN** the gate reports the warning separately and does not classify it as
  an exploitable vulnerability.

### Requirement: Required Evidence Is Fail-Closed

The publication workflow MUST fail when a required coverage, browser, SBOM,
signature, provenance, or smoke artifact is unavailable, stale, or malformed.

#### Scenario: Missing coverage evidence

- **WHEN** the required publication coverage tool cannot run
- **THEN** publication fails with `BLOCKED` evidence and does not upload a
  success report or release artifact.

#### Scenario: Complete evidence

- **WHEN** every required artifact is present and valid for the release commit
- **THEN** the gate returns `PASS` and records the evidence manifest.

### Requirement: Evidence Is Environment-Bound

Every release evidence record MUST identify the commit, workflow or command,
tool versions, target, timestamp, scenario scope, and result state.

#### Scenario: Stale evidence

- **WHEN** an artifact was produced for a different commit or target
- **THEN** the gate rejects it as stale instead of reusing it.
