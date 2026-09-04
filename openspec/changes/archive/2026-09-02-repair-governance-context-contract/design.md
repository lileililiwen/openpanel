# Design: Repair governance context and contract integrity

## Explore & Reuse

- Reuse `openspec/config.yaml` as the declared context source.
- Reuse root `AGENTS.md` and normative `Agents.md`; runtime files must be
  symlinks or content-identical references, not new contracts.
- Reuse `openspec/specs/agent-quality/spec.md` as the behavioral source of
  truth and `scripts/lib/step.sh` for output/exit conventions.

## Approach

Create `scripts/check-agent-governance.sh`. It will fail if required config
context/rules are absent, if `openspec context` emits the empty-context
message, or if a runtime contract is missing, not linked to the canonical
file, or omits the agent-quality spec reference. The script will run through
`make agent-governance` and `make check`; it will be read-only and skip only
when the OpenSpec executable is unavailable, with an explicit status.

The check will use resolved symlink targets and content comparison, avoiding
fragile assumptions about editor-generated files. It will not create `.qoder`;
the set of runtime directories is the set present in the repository and
documented by the contract.

## Verification

Fixture tests cover missing context, empty `openspec context`, broken runtime
link, stale copied contract, and a clean canonical setup. The current
environment’s empty-context result is expected to become a red test until the
OpenSpec configuration/adapter is corrected.

## Approval gate

Implementation requires explicit human approval of this design before
`apply`, as required by `agent-quality`.
