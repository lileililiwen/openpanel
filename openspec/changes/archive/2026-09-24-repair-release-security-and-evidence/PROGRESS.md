# Progress note — repair-release-security-and-evidence (P1)

Session: `ses_f2e7763edffeoxWhM6Ry5M1K6A` (2026-09-24)

## Status

- **Phase**: BFS-1 ✅ → DFS-2 (in progress) → BFS-3 → Phase-4
- **Design approval**: APPROVED as-is by human principal (2026-09-24)
- **Two-commit cadence target**:
  - commit 1 = change artifacts + ticked tasks + archive + manifest ratchet
  - commit 2 = HANDOFF.md follow-up pointer

## Baseline (verified 2026-09-24)

- `rustls 0.23.43` → actionable `RUSTSEC-2026-0285` (TLS 1.3 handshake level
  confusion; patched in `>= 0.23.45`).
- `rustls-pemfile 2.2.0` → unmaintained (informational; RUSTSEC-2025-0134).
- `chacha20 0.10.1` → yanked (informational).
- Existing gate chain up to and including `test-gates`: 71/71 green; only
  the full `make test` step times out the dev session (unrelated, slow test
  suite, not a regression).
- `cargo update -p rustls` already executed: rustls 0.23.43 → 0.23.45,
  rustls-webpki 0.103.13 → 0.103.15; workspace still compiles cleanly.

## Implementation plan

### 1. Dep upgrade (done in worktree, will commit)
- [x] `cargo update -p rustls` → Cargo.lock bumps to rustls 0.23.45,
  rustls-webpki 0.103.15.
- [x] `cargo check --workspace --locked` — clean.

### 2. Audit gate rewrite (TDD)
- [ ] Add failing fixtures to `scripts/test-gates.sh` for the new
      `release-evidence` checker and the rewritten `audit` checker
      (positive + negative each).
- [ ] Rewrite `scripts/check-audit.sh` to:
    - parse `cargo audit --json`,
    - distinguish vulnerabilities (actionable) from informational
      (unmaintained / yanked / notice / unsound),
    - honour `.cargo/audit.toml` `ignore` list (existing),
    - honour `OPENPANEL_AUDIT_REQUIRED` (1 = fail when tool absent),
    - report each finding as a structured line
      (`- [FAIL] <crate> <version> <advisory> -> <remediation>`).
- [ ] Verify fixtures turn green.

### 3. New release-evidence gate (TDD)
- [ ] Add `scripts/check-release-evidence.sh` validating
      `dist/evidence-manifest.json`:
    - Every required record (`audit`, `coverage`, `browser-ui-quality`,
      `release-governance`, `smoke`) present and well-formed.
    - Per-record fields: `id`, `commit`, `command`, `tool_versions`,
      `target`, `timestamp`, `scope`, `state` (PASS|FAIL|BLOCKED).
    - Reject when `commit` ≠ expected commit (stale).
    - Reject when `target` ≠ expected target (stale).
    - Reject any record whose `state` ∈ {`FAIL`,`BLOCKED`} for a
      required record.
    - Reject any record whose `scope` is not a subset of the publication
      scope.
- [ ] Add `make release-evidence` target, wire into `make check`.
- [ ] Verify fixtures turn green.

### 4. CI / release pipeline (BFS-3 cross-surface)
- [ ] Update `.github/workflows/ci.yml`:
    - New `release-audit` job (required) with
      `OPENPANEL_AUDIT_REQUIRED=1`; install `cargo-audit`; run `make audit`.
- [ ] Update `.github/workflows/release.yml`:
    - Generate `dist/evidence-manifest.json` after the release-governance
      gate, recording: commit, ref, target, source_date_epoch, runner
      OS, Rust toolchain version, cargo-audit version, cargo-llvm-cov
      version, axe-core / playwright version (if present), and a record
      for each required gate (audit, coverage-floor, browser-ui-quality,
      release-governance, container smoke).
    - Add a `Required release-evidence gate` step that runs
      `make release-evidence` with `OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1`
      BEFORE the upload step.
    - Block upload on any `BLOCKED` / `FAIL` / missing required record.
- [ ] Update `Makefile` comment block listing the canonical chain so it
      includes `release-evidence`.

### 5. Spec merge
- [ ] Add live spec `openspec/specs/release-evidence/spec.md` containing
      the 3 new requirements (folded from the change delta).
- [ ] Tighten the existing `quality.spec.md` `Dependency Audit`
      requirement to refer to the new strict behavior.
- [ ] Add a single cross-referencing requirement to each of
      `release-deployment-governance.spec.md`,
      `browser-ui-quality.spec.md`, and `quality-maturity-ratchet.spec.md`
      that says their gate output is consumed by the `release-evidence`
      contract (these are the "Modified Capabilities" the proposal lists).

### 6. Governance ratchet
- [ ] Add manifest entries for the 3 new `release-evidence` requirements
      to `openspec/governance/manifest.yaml` so `make governance-contract`
      pins them.

### 7. Phase-4 verification
- [ ] Run focused fixtures via `make test-gates` — all green.
- [ ] Run `make audit release-evidence` directly — green.
- [ ] Run `openspec validate repair-release-security-and-evidence --strict`
      — green.
- [ ] Run `openspec validate --all --strict --no-interactive` — green.
- [ ] Confirm no secrets / full provider responses appear in any
      gate's output (audit redaction check).

### 8. Tick tasks + archive
- [ ] Tick all 11 `tasks.md` check boxes.
- [ ] Run `openspec archive repair-release-security-and-evidence --yes`.

### 9. Two-commit cadence
- [ ] Commit 1: all the above (Cargo.lock bump, scripts, workflows,
      new spec, manifest ratchet, archive folder move, ticked tasks).
- [ ] Commit 2: HANDOFF.md follow-up pointer only.

## Blocker log
(none yet)
