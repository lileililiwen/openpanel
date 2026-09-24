# Design: Repair release security and evidence gates

## Approval

- **Status**: APPROVED as-is by human principal on 2026-09-24
  (session `ses_f2e7763edffeoxWhM6Ry5M1K6A`) as P1 of the portable
  production maturity queue. Per the HANDOFF.md required execution
  protocol, implementation may now proceed through BFS/DFS/BFS/Verification
  phases.

## Approach

Separate the advisory/update work from evidence policy. The dependency graph
and lockfile are the source of dependency truth; CI produces signed release
evidence; local checks may report an unavailable optional tool, while a
publication job MUST fail when its required evidence is absent.

## Explore & Reuse

- Reuse `scripts/check-audit.sh`, `scripts/check-coverage-floor.sh`,
  `scripts/check-browser-ui-quality.sh`, and
  `scripts/check-release-governance.sh`.
- Reuse the existing `release.yml`, browser harness, provenance schema, and
  `governance/evidence.yaml` instead of adding parallel gates.
- Reuse existing audit redaction and CI artifact upload conventions.

## Boundaries and failure behavior

Security advisories are evaluated against the lockfile and approved policy;
unmaintained warnings remain distinct from exploitable vulnerabilities.
Required publication jobs fail closed when coverage, browser, SBOM, signature,
or provenance evidence is unavailable or stale. Developer-only commands retain
clear `SKIPPED` output and never imply release readiness.

## Verification

Test positive and negative fixtures for each gate, run the dependency audit on
the repaired lockfile, execute the complete local quality chain, and verify a
tagged artifact set in an isolated CI-like fixture. Record any provider or
runner limitation as `BLOCKED`, not `PASS`.
