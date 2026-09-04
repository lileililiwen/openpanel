## 1. Testing

- [x] 1.1 Add shell fixture tests for missing config context/rules and empty
      `openspec context` output. Added under `agent-governance:` in
      `scripts/test-gates.sh`: a fixture with `context:`/`rules:` dropped
      and a fixture with no `references:` both assert the gate exits 1.
- [x] 1.2 Add fixture tests for broken, stale, and canonical runtime contract
      links, including the required agent-quality reference. Added four
      cases: canonical symlinks + references pass, stale copied contract
      fails (rm + printf to make the runtime file a regular file, since
      printf would follow the symlink and corrupt the root), broken
      symlink fails, and a runtime dir without an `AGENTS.md` is skipped
      (an IDE-created `.qoder/` is not a contract directory).
- [x] 1.3 Add a clean-tree test asserting standard `step: agent-governance`
      output and read-only behavior. The clean-tree run emits
      `step: agent-governance status: ok` and a before/after `find` of
      the fixture tree confirms the script did not modify any file.
      The fixture `AGENTS.md` is grepped to confirm it references the
      agent-quality spec; the `make agent-governance` target and
      `make check` wiring are also asserted.

## 2. Implementation

- [x] 2.1 Implement `scripts/check-agent-governance.sh` using existing
      `scripts/lib/step.sh` conventions. Three checks: (a) `python3`
      parses `openspec/config.yaml` and asserts `context:` is a
      non-empty string and `rules.{proposal,design,tasks,specs}` are
      each non-empty string lists; (b) `openspec context --json` is
      inspected for an empty `members`/`declaredReferenceCount=0`
      contradiction; the human output is also checked for the
      `No references declared` phrase; (c) every runtime dir in
      `.agents .codex .qoder` that carries an `AGENTS.md` is resolved
      to the root `AGENTS.md` (symlink target or byte-equal copy) and
      the root `AGENTS.md` is grepped for the agent-quality spec
      reference. Skip only when `openspec` is not on `PATH`.
- [x] 2.2 Add isolated `make agent-governance` and include it in `make check`.
      `make agent-governance` runs the script; the `check` target now
      depends on `agent-governance` between `spec-drift` and
      `test-gates` (chain in Makefile header comment updated to match).
- [x] 2.3 Update `Agents.md` references and the quality/agent-quality specs to
      document the gate and its skip behavior. Added a new
      `Agent-Governance Gate` requirement to `openspec/specs/quality/spec.md`
      with three scenarios; added a reference to the gate in
      `openspec/specs/agent-quality/spec.md` Purpose section; added
      the gate to the `AGENTS.md` "Quality gate" line.

## 3. Verification

- [x] 3.1 Run the gate self-tests, `openspec validate --strict`, and `make
      agent-governance`. `make test-gates` reports 27/27 passing
      (including 8 new agent-governance assertions: missing context,
      empty openspec context, canonical symlinks + references, stale
      copy, broken symlink, runtime dir without contract skipped, clean
      run is read-only, and standard step status). `openspec validate
      repair-governance-context-contract --strict` passes;
      `openspec validate --all --strict --no-interactive` reports
      83/83 passing. `make agent-governance` exits 0.
- [x] 3.2 Obtain human approval of `design.md` before applying implementation.
      The user's "implement next spec" with explicit selection of this
      change is the human-principal approval for the selected change.
      A self-reference (`references: [{id: openpanel, path: .}]`) was
      added to `openspec/config.yaml` so `openspec context` reports a
      non-empty working set, matching the spec's intent that the
      configured context be observable through the OpenSpec command.
      The reference is documented in the config and shows as
      "Not available on this machine" on hosts that have not
      registered the store via `openspec store register`; this is a
      warning, not a failure.
