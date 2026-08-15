# testing Specification

## ADDED Requirements

### Requirement: `openspec validate` Enforces Tests-First In tasks.md

The standing rule that every `tasks.md` starts with `## 1. Testing`
before any implementation group SHALL be machine-enforced. `make check`
SHALL run `scripts/check-tasks-testing-first.sh`, which parses every
active change under `openspec/changes/*/tasks.md` and fails if any
non-testing top-level group (e.g. `## 2. Implementation`) appears
before a `## 1. Testing` (or equivalent) group. The script prints
`step: tasks-testing-first status: failed` naming the offending change
and exits non-zero. This closes the "SHOULD be extended ... in a
follow-up change" TODO in the original testing spec.

#### Scenario: Testing group missing or reordered

- **WHEN** a change's `tasks.md` lists `## 2. Implementation` before
  `## 1. Testing`
- **THEN** `make check` fails at the `tasks-testing-first` step and the
  change cannot be applied until reordered.

#### Scenario: Correct order passes

- **WHEN** every active change's `tasks.md` has `## 1. Testing` first
- **THEN** the step prints `step: tasks-testing-first status: ok`.
