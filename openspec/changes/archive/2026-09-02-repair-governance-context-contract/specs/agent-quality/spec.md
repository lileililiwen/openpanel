## ADDED Requirements

### Requirement: Context And Runtime Contract Integrity Gate

The repository SHALL provide `scripts/check-agent-governance.sh` and a
`make agent-governance` target. The gate MUST verify that `openspec/config.yaml`
declares non-empty `context` and all four artifact `rules` blocks, that
`openspec context` does not report an empty reference/context set, and that
each present runtime directory named by the canonical contract loads the same
root `AGENTS.md` contract and references `openspec/specs/agent-quality/spec.md`.
It MUST be read-only and emit the standard step status.

#### Scenario: Empty OpenSpec context is rejected

- **WHEN** `openspec context` reports `No references declared` while the
  project claims a configured context
- **THEN** `agent-governance` fails and names the contradictory output.

#### Scenario: Stale runtime contract is rejected

- **WHEN** `.codex/AGENTS.md` is copied from an older contract and differs from
  root `AGENTS.md`
- **THEN** the gate fails and identifies the runtime path.

#### Scenario: Canonical runtime links pass

- **WHEN** every present runtime contract resolves to or matches root
  `AGENTS.md` and the required agent-quality spec is referenced
- **THEN** the gate exits zero without modifying files.
