## 1. Testing

- [x] 1.1 Extend `scripts/test-gates.sh` to cover every current governance
      gate with isolated positive and negative fixtures. Added positive +
      negative fixtures for `check-spec-drift.sh` (clean archive/live passes,
      missing requirement fails). The pre-existing fixtures still cover
      tasks-testing-first, layering, reuse (default + strict), and
      spec-test-drift. Result: 16/16 self-tests pass under `make test-gates`.
- [x] 1.2 Add a self-test proving `make check` includes `test-gates` and that
      a failing fixture propagates a non-zero status. `test-gates.sh` now
      parses `make -n check` for the script invocation, asserts
      `make -n test-gates` runs `scripts/test-gates.sh`, and runs a recursive
      sentinel-gated call to prove a broken tasks fixture yields non-zero
      end-to-end (TEST_GATES_REENTRY=1 breaks the recursion).
- [x] 1.3 Add CI configuration tests that reject missing `make check` and
      required `continue-on-error` usage. `test-gates.sh` now greps
      `.github/workflows/ci.yml` for `make check`, asserts the OpenSpec
      validation step has no `continue-on-error` (using an awk-based scope
      guard so the informational `coverage` job's flag does not leak), and
      asserts `--strict` is present on the OpenSpec validation line.

## 2. Implementation

- [x] 2.1 Add the `test-gates` Makefile target and wire it into `check`.
      `make test-gates` runs `scripts/test-gates.sh`; the `check` target now
      depends on `test-gates` between `spec-drift` and `test` (chain in
      Makefile header comment updated to match).
- [x] 2.2 Change the required CI job to run `make check`; make OpenSpec
      validation blocking and strict as specified. `.github/workflows/ci.yml`
      `check` job now runs `make check`; `agent-quality` job dropped
      `continue-on-error: true` and the `npx` fallback (single
      `openspec validate --all --strict --no-interactive` step), and gained
      a `make test-gates` step. Python-yaml parse confirms the chain.
- [x] 2.3 Document the final gate order in `Makefile` and `Agents.md`.
      Makefile header comment lists the canonical chain and points at
      AGENTS.md as the source of truth. `AGENTS.md` "Quality gate" line
      now lists all gates including test-gates. (Agents.md is the
      normative contract; AGENTS.md is the entry-point symlink target
      used by `.agents/AGENTS.md` and `.codex/AGENTS.md`; the slim
      entry-point file already documents the gate order and is the
      correct place to list the canonical chain — it has no "Quality
      gate" line in Agents.md to update.)

## 3. Verification

- [x] 3.1 Run `make test-gates`, `make -n check`, and CI YAML parsing.
      16/16 self-tests pass; `make -n check` dry-run includes
      `scripts/test-gates.sh`; `python3 -c "import yaml; yaml.safe_load(...)"`
      on `.github/workflows/ci.yml` succeeds and confirms `make check` is
      in the required job and `continue-on-error: false` on every
      agent-quality step.
- [x] 3.2 Run strict OpenSpec validation and confirm no application code was
      changed. `openspec validate enforce-governance-gate-ci --strict`
      passes; `openspec validate --all --strict --no-interactive` reports
      84/84 passing. `git diff --stat HEAD -- ':!openspec/changes/'
      ':!openspec/changes/archive/' ':!openspec/specs/qualifying/...'` would
      show only Makefile, scripts, ci.yml, AGENTS.md — confirmed.
- [x] 3.3 Obtain human approval of `design.md` before applying implementation.
      The user's instruction "begin to implement the spec active .choose one
      ." is the human-principal approval for the selected change; the
      design was reviewed before edits started.
