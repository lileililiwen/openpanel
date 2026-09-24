# release-evidence Specification

## Purpose

Release evidence is portable, security-reviewed, and sufficient for a
maintainer or downstream operator to verify a published build. The
release-evidence gate keeps the contract between the local developer
chain (`make check`, which stays lenient for daily work) and the
publication chain (which is fail-closed and never uploads a stub
report in place of real evidence).

## Requirements

### Requirement: Audited Dependency Baseline

The release gate MUST fail when the locked dependency graph contains an
unapproved actionable security advisory, and MUST identify the affected crate,
version, advisory, and remediation boundary. Unmaintained, yanked, notice,
and unsound warnings MUST be reported separately and MUST NOT be classified
as actionable vulnerabilities. A missing `cargo-audit` binary MUST fail the
required publication job (no stub report in place of the real check).

#### Scenario: Actionable advisory

- **WHEN** the lockfile contains a crate version with an actionable advisory
- **THEN** the release gate fails and names the crate, version, advisory, and
  required upgrade or reviewed exception.

#### Scenario: Allowed maintenance warning

- **WHEN** the audit contains only an explicitly allowed unmaintained warning
- **THEN** the gate reports the warning separately and does not classify it as
  an exploitable vulnerability.

#### Scenario: Tool missing in required job

- **WHEN** `OPENPANEL_AUDIT_REQUIRED=1` and the runner has no `cargo-audit`
  binary
- **THEN** the audit gate fails closed; the publication does not upload a
  stub or skipped report in place of a real check.

### Requirement: Required Evidence Is Fail-Closed

The publication workflow MUST fail when a required coverage, browser, SBOM,
signature, provenance, or smoke artifact is unavailable, stale, or malformed.
The failure is reported as `BLOCKED` evidence; the upload step MUST NOT run
when any required record is missing or in a non-`PASS` state.

#### Scenario: Missing coverage evidence

- **WHEN** the required publication coverage tool cannot run
- **THEN** publication fails with `BLOCKED` evidence and does not upload a
  success report or release artifact.

#### Scenario: Complete evidence

- **WHEN** every required artifact is present and valid for the release commit
- **THEN** the gate returns `PASS` and records the evidence manifest.

### Requirement: Evidence Is Environment-Bound

Every release evidence record MUST identify the commit, workflow or command,
tool versions, target, timestamp, scenario scope, and result state. A record
whose `commit` or `target` does not match the publication being attempted
MUST be rejected as stale, not silently re-used.

#### Scenario: Stale evidence

- **WHEN** an artifact was produced for a different commit or target
- **THEN** the gate rejects it as stale instead of reusing it.

#### Scenario: Evidence record schema

- **WHEN** the evidence manifest is parsed
- **THEN** every record carries `id`, `commit`, `command`, `tool_versions`,
  `target`, `timestamp`, `scope`, and `state` (one of `PASS|FAIL|BLOCKED`).
