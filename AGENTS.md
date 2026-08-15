# AGENTS.md — Canonical agent contract (entry point)

> **This file is the single entry point for every AI agent runtime**
> (Codex, generic agents, Qoder, Claude Code, etc.). It is kept in
> sync with `Agents.md`, which is the **normative** contract. If they
> ever disagree, `Agents.md` wins. Do not fork agent guidance into
> separate per-tool files — extend `Agents.md` and re-symlink.

## Load order (every session, every runtime)

1. `Agents.md` — the full normative contract (architecture, spec-first
   workflow, TDD, quality engineering, security, anti-patterns).
2. `openspec/specs/agent-quality/spec.md` — the **top-level** capability
   guarding against hallucination, architectural drift, and failure to
   reuse existing logic. This is the system's key loss when relying on
   agents.
3. The OpenSpec change folder you are working in
   (`openspec/changes/<name>/`): `proposal.md`, `design.md`,
   `tasks.md`, and the relevant `openspec/specs/<cap>/spec.md`.

## Top-level guardrails (agent-quality)

- **Global view first.** Read `openspec/config.yaml` `context:` and the
  relevant specs before coding. Never invent an API that already exists.
- **Explore & reuse before implement.** Run `scripts/repo-map.sh` and
  grep for existing utilities/traits/modules; record reuse in
  `design.md`. Duplicating existing logic fails `make check` (reuse gate).
- **Specs are the source of truth.** Code that drifts from a spec is a
  bug. Every `tasks.md` starts with `## 1. Testing` (enforced).
- **Compaction.** Distill progress to a short status after each phase so
  the context window stays focused.
- **Human review gate.** No change is implemented until its `design.md`
  is approved by a human principal.

## Quick reference

- Workflow: `propose → validate → implement (apply) → archive`.
- Quality gate: `make check` (fmt, clippy, docs, audit, reuse, layering,
  spec-test-drift, literal-scan, tests).
- Architecture: strict DDD, four layers; domain is I/O-free; adding a
  bounded context is one `app.register(XModule)` call.
- Repo map: `scripts/repo-map.sh`.
