# Refine agent context-engineering — close the global-view / reuse gaps

## Why

OpenPanel already runs a strong spec-driven, TDD, lint-gated workflow.
But an analysis of how AI coding agents fail at scale (context
pollution, architectural drift, hallucinated APIs, and — most
expensively — **failure to reuse logic that already exists**) showed
that the *mechanisms* OpenSpec and our `Agents.md` provide are only
half-wired:

- OpenSpec's own project-context injection (`config.yaml` `context:` /
  `rules:`) is **commented out**, so every artifact is created without
  the global view.
- Agent guidance lives only in `Agents.md` + `.qoder` skills; `.agents/`
  and `.codex/` are **empty**, and generic runtimes never get the
  canonical instructions.
- The workflow jumps read-specs → plan → implement with **no explicit
  "explore & reuse existing code" step**, which is the direct cause of
  duplicated logic.
- `make check` catches `unwrap`, fmt, file-length, and literal strings,
  but **not** duplication of an existing utility, intra-crate layering
  drift (new bounded contexts are folders, not crates), or spec↔code
  drift.
- The "tests first" rule in `tasks.md` is human-review-only; nothing
  enforces it.

These gaps are the **key loss** of the system: they let an agent
proceed confidently in the wrong direction. They are therefore treated
as a **high-level, cross-cutting** concern, captured in a new top-level
`agent-quality` capability that the other specs reference.

## What Changes

- New high-level capability `agent-quality` (source of truth for how
  agents must work to avoid hallucination / drift / duplication).
- Populate `openspec/config.yaml` `context:` + `rules:` so every
  artifact carries the global view.
- Generate a canonical `AGENTS.md` and share it across `.agents/`,
  `.codex/`, and `.qoder/` so every runtime loads the same contract.
- Add an explicit `## 0. Explore & Reuse` step to the §8 workflow and a
  repo-map tool so agents see existing logic before generating.
- Add context-compaction guidance and a mandatory human review of
  `design.md` before `apply`.
- Extend `quality` with CI gates for reuse/duplication, intra-crate
  layering, and spec↔test drift.
- Extend `testing` so `openspec validate` enforces the testing-first
  rule in `tasks.md`.

## Capabilities

### New Capabilities

- `agent-quality`: cross-cutting guardrails that keep AI agents aligned
  with the global architecture and prevent duplicated / hallucinated
  code. This is the high-level capability; `quality` and `testing` are
  refined to support it.

### Modified Capabilities

- `quality`: add reuse/duplication, intra-crate layering, and
  spec↔test-drift gates to `make check`.
- `testing`: make `openspec validate` enforce the `## 1. Testing`
  before `## 2. Implementation` ordering in `tasks.md`.

## Impact

- Config: `openspec/config.yaml` gains `context:` + `rules:`.
- Docs: new `AGENTS.md`; `Agents.md` §8 gains the explore/reuse step
  and compaction guidance.
- Tooling: `scripts/repo-map.sh`, `scripts/check-tasks-testing-first.sh`,
  `scripts/check-reuse.sh`, `scripts/check-layering.sh`,
  `scripts/check-spec-test-drift.sh`.
- Build: `Makefile` `check` chain and CI gain the new gates (all
  degrade gracefully when optional tooling is absent).
