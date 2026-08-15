# Refine agent context-engineering — Tasks

## 1. Testing

- [x] 1.1 Add a unit test for `scripts/check-tasks-testing-first.sh`:
      a fixture `tasks.md` with `## 2. Implementation` before
      `## 1. Testing` causes a non-zero exit; a correctly ordered one
      passes.
- [x] 1.2 Add a unit test for `scripts/check-layering.sh`: a domain
      file importing `openpanel_app` fails; a clean tree passes.
- [x] 1.3 Add a unit test for `scripts/check-reuse.sh`: two crates
      defining the same `pub fn` fail; a unique definition passes.
- [x] 1.4 Add a unit test for `scripts/check-spec-test-drift.sh`: a
      spec whose capability has a covering test passes; one with no
      covering test is reported.
- [x] 1.5 Add an integration check that `scripts/repo-map.sh` prints a
      non-empty module tree and never modifies source.

## 2. Configuration & Agent Contract (P1, P2, P3, P7, P8)

- [x] 2.1 Populate `openspec/config.yaml` `context:` (tech stack,
      layering, conventions, domain) and `rules:` for `proposal`,
      `tasks`, `specs`, `design`.
- [x] 2.2 Generate/author canonical `AGENTS.md` and reference/symlink
      it from `.agents/` and `.codex/` so all runtimes share the
      contract; verify `openspec update` keeps it in sync.
- [x] 2.3 Add `## 0. Explore & Reuse` step to `Agents.md` §8 workflow.
- [x] 2.4 Add context-compaction guidance to `Agents.md`.
- [x] 2.5 Document the mandatory human review of `design.md` before
      `apply` in `Agents.md` and `proposal.md` workflow.

## 3. Tooling & CI Gates (P4, P5, P6)

- [x] 3.1 Create `scripts/repo-map.sh` (structural module/API map,
      graceful skip).
- [x] 3.2 Create `scripts/check-tasks-testing-first.sh` and wire into
      `Makefile` `check`.
- [x] 3.3 Create `scripts/check-reuse.sh` (duplication heuristic,
      report-by-default / fail-under-`--strict`) and wire `make reuse`
      and `make reuse-strict` into `Makefile`.
- [x] 3.4 Create `scripts/check-layering.sh` (intra-crate DDD boundary)
      and wire into `Makefile` `check`.
- [x] 3.5 Create `scripts/check-spec-test-drift.sh` and wire into
      `Makefile` `check`.
- [x] 3.6 Add CI jobs (or extend existing) to run the new gates,
      degrading gracefully when tooling is absent.

## 4. Validate & Archive

- [x] 4.1 Run `openspec validate refine-agent-context-engineering` and
      fix any format errors.
- [x] 4.2 Run `make check`; confirm all gates (including new) pass or
      skip gracefully.
- [x] 4.3 Run `openspec archive refine-agent-context-engineering` so
      deltas fold into `openspec/specs/{agent-quality,quality,testing}`.
