#!/usr/bin/env bash
# scripts/test-gates.sh — self-test for the agent-quality gates.
#
# Exercises the bash governance gates against tiny isolated fixtures so
# a regression in a gate's logic is caught without a full `make check`.
# Wired into `make check` via the `make test-gates` target so the
# self-test MUST itself be green for `make check` to be green.
#
# Coverage matrix (one positive + one negative fixture per gate):
#   - tasks-testing-first   (tasks.md ordering)
#   - layering              (domain -> app/api forbidden)
#   - reuse                 (cross-crate duplicate, default + strict)
#   - spec-test-drift       (default mode reports, --strict fails)
#   - spec-drift            (archive delta MUST exist in live spec)
#   - scan-literal          (literal colour in a template)
#   - agent-governance      (config context/rules, openspec context, runtime contracts)
#   - governance-contract   (archived governance digest/scenario/checker ratchet)
#   - gate-self-test        (the self-test itself detects a broken gate)
#   - make check integration (test-gates target is wired into check)
#   - ci configuration      (make check + no continue-on-error + --strict)
#
# Checker registry: every gate above registers its positive and negative
# fixture with a `# checker: <id> positive` / `# checker: <id> negative`
# marker. `scripts/check-governance-contract.sh` reads these markers to
# prove each manifest-listed governance requirement maps to a checker
# that really exercises both the compliant and the violating case.
#
# Per spec: "A broken or missing gate target MUST cause a non-zero
# result; the self-test MUST NOT silently skip because a target is
# absent."
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

pass=0; fail=0
assert() { # assert <desc> <expected_exit> <cmd...>
  local desc="$1"; local expected="$2"; shift 2
  if "$@" >/dev/null 2>&1; then rc=0; else rc=$?; fi
  if [ "${rc}" -eq "${expected}" ]; then
    pass=$((pass+1)); echo "  ok   - ${desc}"
  else
    fail=$((fail+1)); echo "  FAIL - ${desc} (expected exit ${expected}, got ${rc})"
  fi
}

# --- tasks-testing-first -------------------------------------------------
# Script globs <CHANGES_DIR>/<change>/tasks.md. Use a SEPARATE dir per
# case so one bad change can't poison the "good" assertion.
TS_GOOD="${TMP}/changes_good"; mkdir -p "${TS_GOOD}/c"
printf '# T\n\n## 1. Testing\n- [ ] x\n\n## 2. Implementation\n- [ ] y\n' > "${TS_GOOD}/c/tasks.md"
# The propagation check below injects a broken tree here: one failing
# gate MUST turn the whole self-test red.
if [ -n "${TEST_GATES_TASKS_GOOD:-}" ]; then
    TS_GOOD="${TEST_GATES_TASKS_GOOD}"
fi
TS_BAD="${TMP}/changes_bad"; mkdir -p "${TS_BAD}/c"
printf '# T\n\n## 2. Implementation\n- [ ] y\n\n## 1. Testing\n- [ ] x\n' > "${TS_BAD}/c/tasks.md"
# checker: tasks-testing-first positive
assert "tasks: correctly ordered passes" 0 env OPENSPEC_CHANGES_DIR="${TS_GOOD}" ./scripts/check-tasks-testing-first.sh
# checker: tasks-testing-first negative
assert "tasks: reordered fails" 1 env OPENSPEC_CHANGES_DIR="${TS_BAD}" ./scripts/check-tasks-testing-first.sh

# --- layering ------------------------------------------------------------
LAY="${TMP}/crates"
mkdir -p "${LAY}/openpanel-domain/src/x" "${LAY}/openpanel-app/src/y"
printf 'use openpanel_app::Foo;\n' > "${LAY}/openpanel-domain/src/x/mod.rs"
printf 'pub fn ok() {}\n' > "${LAY}/openpanel-app/src/y/mod.rs"
# checker: layering negative
assert "layering: domain->app import fails" 1 env OPENSPEC_CRATES_DIR="${LAY}" ./scripts/check-layering.sh
rm "${LAY}/openpanel-domain/src/x/mod.rs"
printf 'pub fn ok() {}\n' > "${LAY}/openpanel-domain/src/x/mod.rs"
# checker: layering positive
assert "layering: clean tree passes" 0 env OPENSPEC_CRATES_DIR="${LAY}" ./scripts/check-layering.sh

# --- reuse ---------------------------------------------------------------
RU="${TMP}/crates2"
mkdir -p "${RU}/openpanel-core/src" "${RU}/openpanel-app/src"
printf 'pub fn compute_widget() {}\n' > "${RU}/openpanel-core/src/lib.rs"
printf 'pub fn compute_widget() {}\n' > "${RU}/openpanel-app/src/lib.rs"
# checker: reuse negative
assert "reuse: cross-crate dup fn fails (strict)" 1 env OPENSPEC_CRATES_DIR="${RU}" ./scripts/check-reuse.sh --strict
assert "reuse: cross-crate dup fn warns (default)" 0 env OPENSPEC_CRATES_DIR="${RU}" ./scripts/check-reuse.sh
printf 'pub fn compute_widget() {}\n' > "${RU}/openpanel-core/src/lib.rs"
printf 'pub fn other_unique() {}\n' > "${RU}/openpanel-app/src/lib.rs"
# checker: reuse positive
assert "reuse: unique names pass" 0 env OPENSPEC_CRATES_DIR="${RU}" ./scripts/check-reuse.sh

# --- spec-test-drift -----------------------------------------------------
# The script resolves its root from its own location and globs
# `tests/**/*.rs` + `crates/**/*.rs`, so a fixture tree is enough.
ST="${TMP}/spec_test_drift"
mkdir -p "${ST}/scripts/lib" "${ST}/openspec/specs/fixturecap" "${ST}/crates/c/src"
cp ./scripts/check-spec-test-drift.sh "${ST}/scripts/check-spec-test-drift.sh"
cp ./scripts/lib/step.sh "${ST}/scripts/lib/step.sh"
printf '# c\n' > "${ST}/crates/c/src/lib.rs"
printf '# fixturecap spec\n\n### Requirement: Fixture Capability\n\n#### Scenario: covered\n\n- **WHEN** a\n- **THEN** b\n' \
  > "${ST}/openspec/specs/fixturecap/spec.md"
# Uncovered: no source file mentions the capability name.
# checker: spec-test-drift negative
assert "spec-test-drift: uncovered capability fails (strict)" 1 "${ST}/scripts/check-spec-test-drift.sh" --strict
printf '// fixturecap coverage marker\n' > "${ST}/crates/c/src/lib.rs"
# checker: spec-test-drift positive
assert "spec-test-drift: covered capability passes (strict)" 0 "${ST}/scripts/check-spec-test-drift.sh" --strict
# Default mode reports gaps without ever failing the build.
printf '# c\n' > "${ST}/crates/c/src/lib.rs"
assert "spec-test-drift: default mode never fails" 0 "${ST}/scripts/check-spec-test-drift.sh"

# --- spec-drift ----------------------------------------------------------
# check-spec-drift.sh resolves its root via `$(dirname "$0")/..`, so to
# fixture it we copy the script into a tmp tree that mirrors
# openspec/changes/archive and openspec/specs.
SD_ROOT="${TMP}/sd"
mkdir -p "${SD_ROOT}/scripts" "${SD_ROOT}/openspec/changes/archive/2026-09-01-fixture/specs/example" \
         "${SD_ROOT}/openspec/specs/example"
cp ./scripts/check-spec-drift.sh "${SD_ROOT}/scripts/check-spec-drift.sh"

# Positive: live spec contains the requirement named in the delta.
printf '## ADDED Requirements\n\n### Requirement: Fixture Requirement\n' \
  > "${SD_ROOT}/openspec/changes/archive/2026-09-01-fixture/specs/example/spec.md"
printf '# example spec\n\n### Requirement: Fixture Requirement\n' \
  > "${SD_ROOT}/openspec/specs/example/spec.md"
# checker: spec-drift positive
assert "spec-drift: clean archive/live passes" 0 "${SD_ROOT}/scripts/check-spec-drift.sh"

# Negative: live spec is missing the requirement the delta introduces.
printf '# example spec\n' > "${SD_ROOT}/openspec/specs/example/spec.md"
# checker: spec-drift negative
assert "spec-drift: missing requirement fails" 1 "${SD_ROOT}/scripts/check-spec-drift.sh"

# Restore live spec for the rest of the suite.
printf '# example spec\n\n### Requirement: Fixture Requirement\n' \
  > "${SD_ROOT}/openspec/specs/example/spec.md"

# --- scan-literal --------------------------------------------------------
# scan-template-literals.sh resolves its root from its own location and
# scans `crates/**/*.{rs,maud,html}`, so a fixture tree is enough.
SL_ROOT="${TMP}/scan_literal"
mkdir -p "${SL_ROOT}/scripts/lib" "${SL_ROOT}/crates/openpanel-web/src"
cp ./scripts/scan-template-literals.sh "${SL_ROOT}/scripts/scan-template-literals.sh"
cp ./scripts/lib/step.sh "${SL_ROOT}/scripts/lib/step.sh"
printf 'pub fn ok() {}\n' > "${SL_ROOT}/crates/openpanel-web/src/lib.rs"
# checker: scan-literal positive
assert "scan-literal: no literals passes" 0 "${SL_ROOT}/scripts/scan-template-literals.sh"
printf 'pub const BRAND: &str = "#4f8cff";\n' > "${SL_ROOT}/crates/openpanel-web/src/lib.rs"
# checker: scan-literal negative
assert "scan-literal: literal hex fails" 1 "${SL_ROOT}/scripts/scan-template-literals.sh"
# The allowlisted token home is exempt from the scan.
mv "${SL_ROOT}/crates/openpanel-web/src/lib.rs" "${SL_ROOT}/crates/openpanel-web/src/tokens.css"
assert "scan-literal: allowlisted token home passes" 0 "${SL_ROOT}/scripts/scan-template-literals.sh"

# --- governance-contract -------------------------------------------------
# Ratchet for archived governance requirements: the manifest pins each
# protected requirement by digest + scenario count and maps it to a
# checker that has both a positive and a negative fixture above.
GC_ROOT="${TMP}/gov_contract"
GC_ARCHIVE="openspec/changes/archive/2026-09-01-fixture/specs/quality/spec.md"
mkdir -p "${GC_ROOT}/scripts/lib" "${GC_ROOT}/openspec/governance" \
         "${GC_ROOT}/openspec/specs/quality" "${GC_ROOT}/openspec/specs/sites" \
         "${GC_ROOT}/openspec/changes/archive/2026-09-01-fixture/specs/quality"
cp ./scripts/check-governance-contract.sh "${GC_ROOT}/scripts/check-governance-contract.sh"
cp ./scripts/lib/step.sh "${GC_ROOT}/scripts/lib/step.sh"
# The checker registry is read from the self-test itself.
cp ./scripts/test-gates.sh "${GC_ROOT}/scripts/test-gates.sh"

write_gov_archive() {
  cat > "${GC_ROOT}/${GC_ARCHIVE}" <<'DELTA'
## ADDED Requirements

### Requirement: Fixture Governance Requirement

#### Scenario: Requirement text is intact

- **WHEN** the live block matches the manifest digest
- **THEN** the governance contract gate exits zero.

#### Scenario: Scenario protection is intact

- **WHEN** both scenarios are present
- **THEN** the scenario count matches the manifest.
DELTA
}

write_gov_live() { # write_gov_live <variant: clean|weakened|scenario-removed>
  {
    echo '# quality — fixture'
    echo ''
    echo '## Requirements'
    echo '### Requirement: Fixture Governance Requirement'
    echo ''
    case "$1" in
      weakened)
        echo 'The fixture requirement documents an intention only.'
        ;;
      *)
        echo 'The fixture requirement MUST stay intact and SHALL NOT be weakened by a'
        echo 'later edit.'
        ;;
    esac
    echo ''
    echo '#### Scenario: Requirement text is intact'
    echo ''
    echo '- **WHEN** the live block matches the manifest digest'
    echo '- **THEN** the governance contract gate exits zero.'
    if [ "$1" != "scenario-removed" ]; then
      echo ''
      echo '#### Scenario: Scenario protection is intact'
      echo ''
      echo '- **WHEN** both scenarios are present'
      echo '- **THEN** the scenario count matches the manifest.'
    fi
  } > "${GC_ROOT}/openspec/specs/quality/spec.md"
}

write_gov_manifest() { # write_gov_manifest <digest> <scenarios> <checkers> [archive]
  local digest="$1"; local scenarios="$2"; local checkers="$3"
  local archive="${4:-${GC_ARCHIVE}}"
  {
    echo 'version: 1'
    echo 'requirements:'
    echo "  - archive: ${archive}"
    echo '    capability: quality'
    echo '    requirement: "Fixture Governance Requirement"'
    echo "    digest: ${digest}"
    echo "    scenarios: ${scenarios}"
    echo "    checkers: [${checkers}]"
  } > "${GC_ROOT}/openspec/governance/manifest.yaml"
}

write_gov_archive
write_gov_live clean
rm -f "${GC_ROOT}/openspec/governance/unprotected-baseline.txt"

# The reviewable digest comes from the gate's own --report mode, so the
# fixture never re-implements (and silently diverges from) the
# normalization used by the check.
GOV_DIGEST="$(env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" \
  "${GC_ROOT}/scripts/check-governance-contract.sh" --report \
  | awk -F'\t' '$3 == "Fixture Governance Requirement" { print $5 }')"
if [ -n "${GOV_DIGEST}" ]; then
  pass=$((pass+1)); echo "  ok   - governance-contract: report prints a digest"
else
  fail=$((fail+1)); echo "  FAIL - governance-contract: report printed no digest"
fi

write_gov_manifest "${GOV_DIGEST}" 2 governance-contract
# checker: governance-contract positive
assert "governance-contract: clean manifest passes" 0 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# A later edit that softens the obligation MUST fail.
write_gov_live weakened
# checker: governance-contract negative
assert "governance-contract: weakened requirement fails" 1 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# Deleting a scenario is a content change, not a silent pass.
write_gov_live scenario-removed
assert "governance-contract: removed scenario fails" 1 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# An archived governance requirement with no manifest entry and no
# baseline entry has no reviewed disposition and MUST fail.
write_gov_live clean
printf 'version: 1\nrequirements: []\n' > "${GC_ROOT}/openspec/governance/manifest.yaml"
assert "governance-contract: missing manifest entry fails" 1 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# An unknown archive path is rejected rather than skipped.
write_gov_manifest "${GOV_DIGEST}" 2 governance-contract \
  "openspec/changes/archive/1999-01-01-does-not-exist/specs/quality/spec.md"
assert "governance-contract: unknown archive path fails" 1 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# A checker ID with no positive/negative fixture is an orphan and MUST fail.
write_gov_manifest "${GOV_DIGEST}" 2 no-such-checker
assert "governance-contract: orphaned checker id fails" 1 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# Back to a clean manifest: an ordinary product capability is outside the
# governance scope, so editing it MUST NOT disturb this gate.
write_gov_manifest "${GOV_DIGEST}" 2 governance-contract
printf '# sites — a product capability, not a governance one\n\n### Requirement: Sites\n\n#### Scenario: s\n\n- **WHEN** a\n- **THEN** b\n' \
  > "${GC_ROOT}/openspec/specs/sites/spec.md"
assert "governance-contract: product spec is out of scope" 0 \
  env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh"

# Diagnostics name the requirement without printing its contents, and a
# clean run never writes to the tree.
write_gov_live weakened
gov_out="$(env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" \
  "${GC_ROOT}/scripts/check-governance-contract.sh" 2>&1 || true)"
if printf '%s' "${gov_out}" | grep -q 'Fixture Governance Requirement'; then
  pass=$((pass+1)); echo "  ok   - governance-contract: diagnostic names the requirement"
else
  fail=$((fail+1)); echo "  FAIL - governance-contract: diagnostic does not name the requirement"
fi
if printf '%s' "${gov_out}" | grep -qF 'SHALL NOT be weakened'; then
  fail=$((fail+1)); echo "  FAIL - governance-contract: diagnostic printed requirement contents"
else
  pass=$((pass+1)); echo "  ok   - governance-contract: diagnostic omits requirement contents"
fi
write_gov_live clean
gov_before="$(find "${GC_ROOT}" \( -type f -o -type l -o -type d \) | sort)"
if env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh" >/dev/null 2>&1; then
  gov_after="$(find "${GC_ROOT}" \( -type f -o -type l -o -type d \) | sort)"
  if [ "${gov_before}" = "${gov_after}" ]; then
    pass=$((pass+1)); echo "  ok   - governance-contract: clean run is read-only"
  else
    fail=$((fail+1)); echo "  FAIL - governance-contract: clean run modified the tree"
  fi
  if env GOVERNANCE_CONTRACT_REPO_ROOT="${GC_ROOT}" "${GC_ROOT}/scripts/check-governance-contract.sh" 2>&1 \
       | grep -qE '^step: governance-contract status: ok$'; then
    pass=$((pass+1)); echo "  ok   - governance-contract: emits standard step status"
  else
    fail=$((fail+1)); echo "  FAIL - governance-contract: missing standard step status"
  fi
else
  fail=$((fail+1)); echo "  FAIL - governance-contract: clean fixture should pass"
fi

# --- agent-governance ----------------------------------------------------
# Per the agent-quality spec: `scripts/check-agent-governance.sh` MUST
# verify that openspec/config.yaml has a non-empty context and all four
# rules blocks, that `openspec context` does not report an empty
# reference/context set, and that every present runtime contract
# (`.agents/`, `.codex/`, `.qoder/`) resolves to the root AGENTS.md and
# references the agent-quality spec.
#
# The script reads its repo root from AGENT_GOVERNANCE_REPO_ROOT so a
# fixture tree is enough to isolate the test from the real repo
# (whose openspec context is currently empty, by design — the gate
# will be red there until the OpenSpec configuration/adapter is
# corrected).
AG_ROOT="${TMP}/agent_gov"
AG_SCRIPTS="${AG_ROOT}/scripts/lib"
mkdir -p "${AG_SCRIPTS}" "${AG_ROOT}/.agents" "${AG_ROOT}/.codex" \
         "${AG_ROOT}/openspec" "${AG_ROOT}/openspec/specs/agent-quality"
cp ./scripts/check-agent-governance.sh "${AG_ROOT}/scripts/check-agent-governance.sh"
cp ./scripts/lib/step.sh "${AG_SCRIPTS}/step.sh"
# A canonical AGENTS.md that references the required agent-quality spec.
cat > "${AG_ROOT}/AGENTS.md" <<AGENTS
# AGENTS — canonical contract for the fixture.
- See: openspec/specs/agent-quality/spec.md
AGENTS
ln -s "${AG_ROOT}/AGENTS.md" "${AG_ROOT}/.agents/AGENTS.md"
ln -s "${AG_ROOT}/AGENTS.md" "${AG_ROOT}/.codex/AGENTS.md"
touch "${AG_ROOT}/openspec/specs/agent-quality/spec.md"

write_config() { # write_config <root> <has_context> <has_all_rules> <has_refs>
  local root="$1"; local has_ctx="$2"; local has_rules="$3"; local has_refs="$4"
  {
    echo "schema: spec-driven"
    if [ "${has_ctx}" = "1" ]; then
      echo 'context: |'
      echo '  Configured project context for the fixture.'
    fi
    if [ "${has_rules}" = "1" ]; then
      echo 'rules:'
      for k in proposal design tasks specs; do
        echo "  ${k}:"
        echo "    - rule for ${k}"
      done
    else
      # Drop one of the four rules so the gate must fail.
      echo 'rules:'
      for k in proposal design tasks; do
        echo "  ${k}:"
        echo "    - rule for ${k}"
      done
    fi
    if [ "${has_refs}" = "1" ]; then
      # A reference that does not resolve to a registered store still
      # populates `members` in `openspec context --json`, so the
      # "empty reference/context set" check passes.
      echo 'references:'
      echo '  - id: self'
      echo '    path: .'
    fi
  } > "${root}/openspec/config.yaml"
}

# 1.1 missing config context/rules and empty openspec context
write_config "${AG_ROOT}" 0 0 1
assert "agent-governance: missing context fails" 1 \
  env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
      AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
      "${AG_ROOT}/scripts/check-agent-governance.sh"
write_config "${AG_ROOT}" 1 1 0
# No references => "empty reference/context set" => gate must fail.
assert "agent-governance: empty openspec context fails" 1 \
  env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
      AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
      "${AG_ROOT}/scripts/check-agent-governance.sh"

# 1.2 broken, stale, and canonical runtime contract links
write_config "${AG_ROOT}" 1 1 1
# checker: agent-governance positive
assert "agent-governance: canonical symlinks + references pass" 0 \
  env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
      AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
      "${AG_ROOT}/scripts/check-agent-governance.sh"
# Stale copy: replace the symlink with a divergent regular file. The
# rm MUST come first — otherwise the printf would follow the symlink
# and corrupt the root AGENTS.md instead of producing a stale .codex
# copy.
rm -f "${AG_ROOT}/.codex/AGENTS.md"
printf 'old contract from yesterday\n' > "${AG_ROOT}/.codex/AGENTS.md"
# checker: agent-governance negative
assert "agent-governance: stale copied contract fails" 1 \
  env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
      AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
      "${AG_ROOT}/scripts/check-agent-governance.sh"
# Broken symlink: target does not exist.
rm -f "${AG_ROOT}/.codex/AGENTS.md"
ln -s /nonexistent/AGENTS.md "${AG_ROOT}/.codex/AGENTS.md"
assert "agent-governance: broken symlink fails" 1 \
  env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
      AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
      "${AG_ROOT}/scripts/check-agent-governance.sh"
# Restore canonical state for the read-only check.
rm -f "${AG_ROOT}/.codex/AGENTS.md"
ln -s "${AG_ROOT}/AGENTS.md" "${AG_ROOT}/.codex/AGENTS.md"

# A runtime dir that exists but does NOT carry an AGENTS.md (e.g. an
# IDE-created .qoder/) is not a contract directory and MUST be
# skipped, not failed.
mkdir -p "${AG_ROOT}/.qoder"
write_config "${AG_ROOT}" 1 1 1
assert "agent-governance: runtime dir without contract is skipped" 0 \
  env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
      AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
      "${AG_ROOT}/scripts/check-agent-governance.sh"
rmdir "${AG_ROOT}/.qoder"

# 1.3 clean tree test: standard step status, read-only behavior.
write_config "${AG_ROOT}" 1 1 1
snap_before="$(find "${AG_ROOT}" -type f -o -type l -o -type d | sort)"
out="$(env AGENT_GOVERNANCE_REPO_ROOT="${AG_ROOT}" \
        AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder" \
        "${AG_ROOT}/scripts/check-agent-governance.sh" 2>&1)"
snap_after="$(find "${AG_ROOT}" \( -type f -o -type l -o -type d \) | sort)"
if [ "${snap_before}" = "${snap_after}" ]; then
  pass=$((pass+1)); echo "  ok   - agent-governance: clean run is read-only"
else
  fail=$((fail+1)); echo "  FAIL - agent-governance: clean run modified the tree"
fi
if printf '%s' "${out}" | grep -qE '^step: agent-governance status: ok$'; then
  pass=$((pass+1)); echo "  ok   - agent-governance: emits standard step status"
else
  fail=$((fail+1)); echo "  FAIL - agent-governance: missing standard step status"
  printf '   output:\n%s\n' "${out}" | sed 's/^/   /'
fi
# The canonical AGENTS.md MUST reference the agent-quality spec, or
# the script's earlier positive case would not be reachable.
if grep -qF 'openspec/specs/agent-quality/spec.md' "${AG_ROOT}/AGENTS.md"; then
  pass=$((pass+1)); echo "  ok   - agent-governance: fixture references agent-quality spec"
else
  fail=$((fail+1)); echo "  FAIL - agent-governance: fixture AGENTS.md missing the spec reference"
fi

# 1.3b the make target exists and is wired to the script.
if command -v make >/dev/null 2>&1; then
  if make -n agent-governance 2>/dev/null | grep -q 'scripts/check-agent-governance.sh'; then
    pass=$((pass+1)); echo "  ok   - make agent-governance target runs the script"
  else
    fail=$((fail+1)); echo "  FAIL - make agent-governance target missing or unwired"
  fi
  if make -n check 2>/dev/null | grep -q 'scripts/check-agent-governance.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes agent-governance"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include agent-governance"
  fi
else
  fail=$((fail+1)); echo "  FAIL - make unavailable, cannot verify agent-governance wiring"
fi

# --- make check integration ----------------------------------------------
# Spec: "`make check` SHALL run [test-gates]." Prove the dependency
# chain is wired without actually running the heavy gates.
if command -v make >/dev/null 2>&1; then
  plan="$(cd "${REPO_ROOT}" && make -n check 2>/dev/null || true)"
  if printf '%s' "${plan}" | grep -q 'scripts/test-gates.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes test-gates"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include test-gates"
  fi

  # The isolated `make test-gates` target MUST exist and be wired to
  # the script (per spec: orphaned self-test is rejected).
  if make -n test-gates 2>/dev/null | grep -q 'scripts/test-gates.sh'; then
    pass=$((pass+1)); echo "  ok   - make test-gates target runs the script"
  else
    fail=$((fail+1)); echo "  FAIL - make test-gates target missing or unwired"
  fi
else
  fail=$((fail+1)); echo "  FAIL - make unavailable, cannot verify wiring"
fi

# The governance-contract gate needs the same isolated-target treatment:
# an unwired ratchet is no ratchet at all.
if command -v make >/dev/null 2>&1; then
  if make -n governance-contract 2>/dev/null | grep -q 'scripts/check-governance-contract.sh'; then
    pass=$((pass+1)); echo "  ok   - make governance-contract target runs the script"
  else
    fail=$((fail+1)); echo "  FAIL - make governance-contract target missing or unwired"
  fi
  if make -n check 2>/dev/null | grep -q 'scripts/check-governance-contract.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes governance-contract"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include governance-contract"
  fi
fi

# --- ci configuration ----------------------------------------------------
# Required checks per spec:
#   - CI runs `make check` on every push/PR
#   - required steps MUST NOT use `continue-on-error`
#   - OpenSpec validation MUST use --strict
CI_FILE="${REPO_ROOT}/.github/workflows/ci.yml"
if [ -f "${CI_FILE}" ]; then
  if grep -qE 'make[[:space:]]+check' "${CI_FILE}"; then
    pass=$((pass+1)); echo "  ok   - ci runs make check"
  else
    fail=$((fail+1)); echo "  FAIL - ci does NOT run make check"
  fi

  # `continue-on-error: true` is only allowed on informational steps
  # (e.g. coverage). The OpenSpec validation step MUST NOT have it.
  if awk '
      /continue-on-error:[[:space:]]*true/ { fail=1 }
      /openspec.*validate/                 { have_os=1; if (fail) { print "OPNSPEC"; exit 1 } }
      /coverage/                           { fail=0 }
    ' "${CI_FILE}" | grep -q OPNSPEC; then
    fail=$((fail+1)); echo "  FAIL - ci has continue-on-error on openspec validation"
  else
    pass=$((pass+1)); echo "  ok   - ci openspec step has no continue-on-error"
  fi

  if grep -qE 'openspec.*validate.*--strict' "${CI_FILE}"; then
    pass=$((pass+1)); echo "  ok   - ci openspec validation is --strict"
  else
    fail=$((fail+1)); echo "  FAIL - ci openspec validation missing --strict"
  fi
else
  fail=$((fail+1)); echo "  FAIL - ci workflow file missing"
fi

# --- propagation: a failing fixture must yield non-zero overall ---------
# Spec: "self-test MUST NOT silently skip because a target is absent."
# Run the script recursively with a broken fixture; the inner call MUST
# exit non-zero. The TEST_GATES_REENTRY sentinel breaks infinite
# recursion (the inner call skips this block).
if [ -z "${TEST_GATES_REENTRY:-}" ]; then
  # A healthy tree yields a green self-test; this is the positive half of
  # the `gate-self-test` checker (the negative half is the broken
  # fixture below).
  # checker: gate-self-test positive
  assert "gate-self-test: clean self-test run passes" 0 \
    env TEST_GATES_REENTRY=1 ./scripts/test-gates.sh

  # The broken tree is injected through TEST_GATES_TASKS_GOOD so a real
  # assertion fails inside the inner run — the self-test must propagate
  # a gate failure, not swallow it.
  # checker: gate-self-test negative
  BAD_TS_DIR="${TMP}/propagation"
  mkdir -p "${BAD_TS_DIR}/c"
  printf '# T\n\n## 2. Implementation\n- [ ] y\n' > "${BAD_TS_DIR}/c/tasks.md"
  if env TEST_GATES_TASKS_GOOD="${BAD_TS_DIR}" TEST_GATES_REENTRY=1 \
       ./scripts/test-gates.sh >/dev/null 2>&1; then
    fail=$((fail+1)); echo "  FAIL - propagation: broken tasks fixture should yield non-zero"
  else
    pass=$((pass+1)); echo "  ok   - propagation: broken tasks fixture yields non-zero"
  fi
else
  # Inner call: the tasks gate MUST reject a tasks.md whose first group
  # is not the testing group. Fall back to a local broken fixture so the
  # assertion can never pass vacuously on a missing directory.
  REENTRY_DIR="${TMP}/reentry_broken"
  mkdir -p "${REENTRY_DIR}/c"
  printf '# T\n\n## 2. Implementation\n- [ ] y\n' > "${REENTRY_DIR}/c/tasks.md"
  assert "propagation (reentry): broken tasks fails" 1 \
    env OPENSPEC_CHANGES_DIR="${REENTRY_DIR}" ./scripts/check-tasks-testing-first.sh
fi

echo ""
echo "gate self-tests: ${pass} passed, ${fail} failed"
[ "${fail}" -eq 0 ]
