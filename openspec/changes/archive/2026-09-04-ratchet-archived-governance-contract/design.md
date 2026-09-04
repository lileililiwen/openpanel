# Design: Ratchet archived governance contract

## Explore & Reuse

- Reuse `scripts/check-spec-drift.sh` for existing archive/live heading
  compatibility and its baseline policy.
- Reuse `scripts/test-gates.sh` as the executable checker registry and
  `scripts/lib/step.sh` for gate output.
- Reuse archived deltas under `openspec/changes/archive/` and live specs under
  `openspec/specs/`; no second specification store will be introduced.

## Approach

Add `openspec/governance/manifest.yaml` entries containing archive path,
capability, exact requirement name, normalized content digest, scenario count,
and one or more checker IDs. Add `scripts/check-governance-contract.sh` to
extract each named requirement block from the archived and live specs, compare
the approved digest and scenario count, and verify every checker ID is exposed
by the gate self-test. It will reject missing manifest entries for newly
archived governance requirements and reject unknown archive/spec paths.

The manifest is updated only by a reviewed OpenSpec change. A requirement may
be intentionally changed only by adding a new delta that updates the manifest;
the gate never rewrites it. Diagnostics name the archive, capability,
requirement, and missing checker, without printing full spec contents.

## Verification

Fixture tests cover a changed MUST sentence, removed scenario, missing
manifest entry, unknown checker ID, and a clean manifest. Tests also verify
that changing an ordinary product spec does not trigger this focused gate.

## Approval gate

Implementation requires explicit human approval of `design.md` before
`apply`.
