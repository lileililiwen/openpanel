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
- **Governance cannot be softened silently.** Archived requirements in
  `agent-quality` / `quality` / `testing` / `architecture` are pinned by
  `openspec/governance/manifest.yaml`; changing one requires updating
  its digest in the same reviewed change (see `Agents.md` §5.1).
- **Compaction.** Distill progress to a short status after each phase so
  the context window stays focused.
- **Human review gate.** No change is implemented until its `design.md`
  is approved by a human principal.

## Quick reference

- Workflow: `propose → validate → implement (apply) → archive`.
- Per-change commits: a change produces **two commits** in order —
  (1) the change itself (implementation + ticked `tasks.md` +
  archived spec deltas + manifest ratchet + archive folder move),
  then (2) the `HANDOFF.md` follow-up that records the commit-1
  hash. Do not amend or merge them; the split keeps commit 1 a
  clean "what this change did" commit. See `HANDOFF.md`
  "Two-commit cadence per change" for the full procedure.
- Quality gate: `make check` (fmt, clippy, docs, audit, file-length,
  scan-literal, class-coverage, tasks-testing-first, reuse, layering,
  spec-test-drift, spec-drift, agent-governance, governance-contract,
  test-gates, tests). `make test-gates` runs the governance
  self-test in isolation. `make agent-governance` re-verifies the
  OpenSpec context and every runtime contract link.
  `make governance-contract` re-verifies the archived-governance
  manifest (content digest, scenario count, checker mapping); see
  `Agents.md` §5.1 for the reviewed update procedure.
- Architecture: strict DDD, four layers; domain is I/O-free; adding a
  bounded context is one `app.register(XModule)` call.
- Repo map: `scripts/repo-map.sh`.
