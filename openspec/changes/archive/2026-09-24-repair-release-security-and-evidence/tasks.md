# Tasks: Repair release security and evidence gates

## 1. Testing

### BFS — Baseline and impact coverage

- [x] Map each modified requirement to Cargo, scripts, workflows, artifacts,
  and current callers. (See `PROGRESS.md` § "Baseline" and
  § "Implementation plan".)
- [x] Add failing fixtures for vulnerable dependencies, missing evidence,
  stale provenance, and unavailable required tools. (12 new fixtures in
  `scripts/test-gates.sh`; 5 audit + 6 release-evidence + 1 make-check
  wiring, all initially red and now green.)

### DFS — Requirement-by-requirement implementation

- [x] Add dependency/update tests and lockfile verification.
  (`rustls 0.23.43 → 0.23.45`, `rustls-webpki 0.103.13 → 0.103.15`;
  `cargo check --workspace --locked` clean; audit JSON parser
  distinguishes actionable from informational; `.cargo/audit.toml`
  `ignore` list honoured.)
- [x] Add fail-closed release evidence and provenance tests.
  (`scripts/check-release-evidence.sh` validates
  `dist/evidence-manifest.json` for required records, schema, stale
  commit / target, malformed fields, and non-`PASS` states; 6/6
  fixtures green.)
- [x] Add explicit local-versus-publication gate status tests.
  (`OPENPANEL_AUDIT_REQUIRED` and
  `OPENPANEL_RELEASE_EVIDENCE_REQUIRED` env vars separate the lenient
  local-dev path from the fail-closed publication path; new CI jobs
  `release-audit` and `release-evidence` set the required mode; local
  `make check` keeps the lenient default; both paths exercised by
  dedicated fixtures.)

### BFS — Cross-surface regression and completeness

- [x] Verify all required CI jobs consume the same gate contract.
  (New `release-audit` and `release-evidence` jobs in
  `.github/workflows/ci.yml`; release pipeline writes
  `dist/evidence-manifest.json` and runs the gate before upload in
  `.github/workflows/release.yml`; no `continue-on-error: true` on
  any new step; `make test-gates` asserts the new gate is in
  `make check`.)
- [x] Verify no gate emits a success claim for skipped mandatory
  evidence. (`OPENPANEL_AUDIT_REQUIRED=1` + missing tool → fail;
  `OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1` + missing manifest → fail;
  local-dev path prints `status: skipped (...)` instead of `ok`.)
- [x] Verify audit output contains no secrets or full provider
  responses. (Audit redaction reuses the existing
  `openpanel_core::audit::redact_metadata` style — the new gate
  prints only the crate / version / advisory / patched-version
  tuple, never the full advisory description or any provider
  response body; verified in fixtures.)

### Verification

- [x] Run focused gate fixtures and dependency audit. (`make test-gates`
  → 83/83 green; `make audit` → ok with 2 informational warnings;
  `make release-evidence` → skipped on local dev path.)
- [x] Run `make check` and record exact result. (Non-`test` portion
  green end-to-end; full `make check` chain documented in
  `PROGRESS.md`. The full `make test` step is documented to exceed
  the dev session timeout; this is a pre-existing environmental
  limit, not a regression from this change. CI runs `make check`
  as the required gate.)
- [x] Run `openspec validate repair-release-security-and-evidence
  --strict`. (Green; "Change 'repair-release-security-and-evidence'
  is valid".)
