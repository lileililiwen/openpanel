## 1. Testing

- [x] 1.1 Add fixtures for a weakened requirement block and a removed
      scenario; assert the content gate fails.
- [x] 1.2 Add fixtures for missing manifest entries, unknown archive paths,
      and orphan checker IDs; assert each fails with a named diagnostic.
- [x] 1.3 Add a clean-manifest fixture and verify ordinary product specs are
      outside the focused governance scope.
- [x] 1.4 Extend gate self-tests so every mapped checker has positive and
      negative assertions.

## 2. Implementation

- [x] 2.1 Create `openspec/governance/manifest.yaml` from the archived
      governance requirements after reviewing archive/live differences.
- [x] 2.2 Implement `scripts/check-governance-contract.sh` with normalized
      requirement extraction, digest/scenario comparison, and checker-ID
      validation.
- [x] 2.3 Wire an isolated Make target and `make check`; document the reviewed
      manifest update procedure in `Agents.md`.

## 3. Verification

- [x] 3.1 Run the focused gate, its self-tests, and strict OpenSpec validation.
- [x] 3.2 Confirm diagnostics do not print complete requirement contents or
      secrets and that the check is read-only.
- [x] 3.3 Obtain human approval of `design.md` before applying implementation.
