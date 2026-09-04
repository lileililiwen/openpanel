# Proposal: Repair governance context and contract integrity

## Why

The archived `refine-agent-context-engineering` change made the project
context and one canonical agent contract mandatory, but the current
`openspec context` output says no references are declared. Runtime contract
links are also not checked mechanically, so a later edit can silently split
agent guidance.

## What

Add a governance-integrity check that validates the configured context is
observable through the OpenSpec command and that every declared agent runtime
loads the canonical contract and agent-quality spec. Reuse `openspec/config.yaml`,
`AGENTS.md`, `Agents.md`, and the existing `scripts/lib/step.sh` protocol.

## Capabilities

### New

- `agent-quality`: context and canonical-contract integrity gate.

### Modified

- `quality`: mandatory Makefile entry for the new gate.

## Non-goals

- No application-code changes.
- No automatic rewriting of agent files.
- No change to the authority rule that `Agents.md` is normative.
