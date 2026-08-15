# Refine agent context-engineering — Design

## Problem framing

The failure modes we are defending against:

| Failure mode | Mechanism this change adds |
|---|---|
| Hallucination (wrong APIs/paths) | `config.yaml` context injection + `make check` (compile/test as ground truth) + mandatory human review of `design.md` |
| Lost global view / architectural drift | `agent-quality` capability as source of truth + repo-map + compaction guidance + intra-crate layering gate |
| Failure to reuse existing logic | explicit `## 0. Explore & Reuse` step + `scripts/repo-map.sh` + reuse/duplication gate |
| Duplication / slop | reuse/duplication gate + repo-map surfacing existing utilities |

## Layering of the new capability

`agent-quality` is deliberately placed at the **top level** (a
first-class capability alongside `architecture`, `quality`,
`testing`). It is the contract agents load first; the other specs are
refinements that operationalise it. No code in the Rust workspace
depends on it — it is documentation + tooling only, so it carries no
layering risk.

## Tooling design

All new gates follow the existing pattern from `scripts/lib/step.sh`
and degrade gracefully (print `status: skipped` and `exit 0`) when the
underlying tooling is absent, mirroring the `audit` and `file-length`
gates.

- **`scripts/repo-map.sh`** — emits a tree of modules + their public
  APIs (cheap: `cargo doc --no-deps` outline + `rg` of `pub fn`/
  `pub struct`/`pub trait`). Referenced from `Agents.md` so an agent
  builds a structural map before editing.
- **`scripts/check-tasks-testing-first.sh`** — parses every active
  change's `tasks.md`; fails if `## 2. Implementation` (or any
  non-testing group) appears before `## 1. Testing`. This makes the
  long-standing standing rule machine-enforced.
- **`scripts/check-reuse.sh`** — heuristic duplication detector: flags
  the same `pub fn`/`pub trait` name defined in more than one crate
  (excluding tests), prompting the author to reuse the existing one.
- **`scripts/check-layering.sh`** — enforces intra-crate DDD
  boundaries that cargo cannot: `openpanel-domain/*` must not `use
  openpanel_app`/`openpanel_api`; `openpanel-app/*` must not `use
  openpanel_api`. Folders (not crates) are the unit, so this is a
  path-based `rg` check.
- **`scripts/check-spec-test-drift.sh`** — for every archived spec
  scenario (`#### Scenario:`), asserts at least one test references
  the capability; reports gaps without failing the build on
  pre-existing specs (opt-in strict mode for new specs).

## Agent-instruction unification

`openspec update` regenerates the canonical instruction file
(`AGENTS.md`). We author `AGENTS.md` to point at `Agents.md` and the
new `agent-quality` spec, then place a symlink/reference in `.agents/`
and `.codex/` so Codex and generic runtimes receive the same contract
as `.qoder`. This removes the current divergence where only Qoder had
skills.

## Validation

`openspec validate refine-agent-context-engineering` must pass;
`make check` must pass with the new gates active (graceful skip when
tools absent). Archive folds the deltas into
`openspec/specs/{agent-quality,quality,testing}/spec.md`.
