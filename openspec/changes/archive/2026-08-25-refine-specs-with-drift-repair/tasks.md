# Refine specs with drift repair — Tasks

## 1. Verification (docs-only change)

- [x] 1.1 Diff check: every requirement name in each merged delta
      appears exactly once in the corresponding live spec after merge
      (`rg -c '^### Requirement: <name>'` == 1 per spec).
- [x] 1.2 Scenario-count check: merged requirements retain all their
      `#### Scenario:` blocks (count before == count after).
- [x] 1.3 Gate self-test: remove one merged requirement from a scratch
      copy of `openspec/specs/dns/spec.md` and confirm
      `scripts/check-spec-drift.sh` exits 1 naming the missing
      requirement; restore afterwards.
- [x] 1.4 `openspec validate` passes for both capabilities.

## 2. Merges

- [x] 2.1 Fold the five DNSSEC/secondary-DNS requirements into
      `openspec/specs/dns/spec.md`.
- [x] 2.2 Fold the three database-privilege requirements into
      `openspec/specs/databases/spec.md`.

## 3. Gate

- [x] 3.1 Implement `scripts/check-spec-drift.sh` (ratchet model:
      `.drift-baseline` tracks pre-existing gaps, new drift fails).
- [x] 3.2 Generate the initial baseline (106 entries across 11
      capabilities) and commit it.
- [x] 3.3 Wire `spec-drift` into `make check` (Makefile).

## 4. Follow-up (separate changes, tracked debt)

- [ ] 4.1 Per-capability review of remaining baseline entries:
      distinguish "never merged" from "superseded by rewrite"; merge
      or delete accordingly, shrinking the baseline to zero.

## 5. Validation

- [x] 5.1 `make spec-drift` clean (0 new drift).
- [ ] 5.2 Archive with `openspec archive refine-specs-with-drift-repair`.
