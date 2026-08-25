# Refine specs with drift repair — Design

## Explore & Reuse

- Archived deltas to merge verbatim:
  - `openspec/changes/archive/2026-08-15-2026-08-14-add-dnssec-and-secondary-dns/specs/dns/spec.md`
    (5 ADDED requirements)
  - `openspec/changes/archive/2026-08-15-2026-08-14-add-database-privilege-management/specs/databases/spec.md`
    (3 ADDED requirements)
- Live targets: `openspec/specs/dns/spec.md`,
  `openspec/specs/databases/spec.md`.
- Gate placement: `openspec/specs/quality/spec.md` already owns
  "Spec-To-Test Drift Gate"; the new requirement extends the same
  section family and is enforced in `scripts/check-tests.sh`'s
  sibling scripts (new `scripts/check-spec-drift.sh`) wired into
  `make check` next to the existing gates.
- Evidence of drift:
  `crates/openpanel-app/src/lib.rs` exports `DnsSecSecondaryModule`,
  `GlueRecordService`, `AxfrSender`, db-privilege services; greps show
  zero matching requirements in the live specs.

## Merge procedure

1. Append each archived requirement block (heading + body + scenarios)
   under `## Requirements` of the live spec, preserving order from the
   delta.
2. Run `openspec validate` on both capabilities.
3. Cross-check each merged requirement against existing tests
   (`rg 'RequirementName|scenario phrase' tests/ crates/`) and record
   coverage notes inline as HTML comments — no test changes here.

## New gate (ratchet)

```
check-spec-drift.sh [--write-baseline]:
  for change in openspec/changes/archive/*/:
      for delta in change/specs/<cap>/spec.md:
          for req in ADDED|MODIFIED requirement names:
              if "### Requirement: <req>" not in live spec:
                  emit "<cap>\t<req>\t<delta-path>"
  --write-baseline: sorted unique "<cap>\t<req>" -> .drift-baseline
  check: entries in baseline  -> report only ("drift(baseline):")
         entries not in base  -> DRIFT, exit 1
```

POSIX sh + awk + grep; no new dependencies. Baseline committed at
`openspec/specs/.drift-baseline` (106 initial entries) so local runs
and CI agree. `make spec-drift` runs it; wired into `make check`
between `spec-test-drift` and `test`. Baseline shrinks as merges land;
it must never grow.

## Layering

No runtime code. Scripts + specs only; domain/app/api untouched.
