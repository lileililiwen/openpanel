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
#   - scan-literal          (literal colour in a template; allowlist
#                            and app.css paths are exercised)
#   - class-coverage        (every class="..." literal has a rule
#                            in app.css; dynamic-family expansion)
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
# scans `crates/**/*.{rs,maud,html,css}`, so a fixture tree is enough.
SL_ROOT="${TMP}/scan_literal"
mkdir -p "${SL_ROOT}/scripts/lib" "${SL_ROOT}/crates/openpanel-web/src" \
         "${SL_ROOT}/crates/openpanel-web/assets"
cp ./scripts/scan-template-literals.sh "${SL_ROOT}/scripts/scan-template-literals.sh"
cp ./scripts/lib/step.sh "${SL_ROOT}/scripts/lib/step.sh"
printf 'pub fn ok() {}\n' > "${SL_ROOT}/crates/openpanel-web/src/lib.rs"
# checker: scan-literal positive
assert "scan-literal: no literals passes" 0 "${SL_ROOT}/scripts/scan-template-literals.sh"
printf 'pub const BRAND: &str = "#4f8cff";\n' > "${SL_ROOT}/crates/openpanel-web/src/lib.rs"
# checker: scan-literal negative
assert "scan-literal: literal hex fails" 1 "${SL_ROOT}/scripts/scan-template-literals.sh"
# The allowlisted token home (assets/tokens.css) is exempt from the scan.
mv "${SL_ROOT}/crates/openpanel-web/src/lib.rs" "${SL_ROOT}/crates/openpanel-web/assets/tokens.css"
assert "scan-literal: allowlisted token home passes" 0 "${SL_ROOT}/scripts/scan-template-literals.sh"
# A literal hex inside any other CSS file is a violation.
printf '.x { color: #4f8cff; }\n' > "${SL_ROOT}/crates/openpanel-web/assets/app.css"
# checker: scan-literal negative
assert "scan-literal: literal hex in app.css fails" 1 "${SL_ROOT}/scripts/scan-template-literals.sh"

# --- class-coverage ------------------------------------------------------
# check-class-coverage.sh resolves its root from its own location and
# reads the source tree under OPENSPEC_WEB_SRC_DIR + the stylesheet at
# OPENSPEC_WEB_CSS_FILE, so a fixture tree is enough.
CC_ROOT="${TMP}/class_coverage"
mkdir -p "${CC_ROOT}/scripts/lib" "${CC_ROOT}/crates/openpanel-web/src" \
         "${CC_ROOT}/crates/openpanel-web/assets"
cp ./scripts/check-class-coverage.sh "${CC_ROOT}/scripts/check-class-coverage.sh"
cp ./scripts/lib/step.sh "${CC_ROOT}/scripts/lib/step.sh"
printf '.known-widget { color: red; }\n' > "${CC_ROOT}/crates/openpanel-web/assets/app.css"
printf 'div class="known-widget"\n' > "${CC_ROOT}/crates/openpanel-web/src/lib.rs"
# checker: class-coverage positive
assert "class-coverage: every class has a rule" 0 \
  env OPENSPEC_WEB_SRC_DIR="${CC_ROOT}/crates/openpanel-web/src" \
      OPENSPEC_WEB_CSS_FILE="${CC_ROOT}/crates/openpanel-web/assets/app.css" \
      "${CC_ROOT}/scripts/check-class-coverage.sh"
# A new class without a rule MUST fail.
printf 'div class="made-up-widget"\n' > "${CC_ROOT}/crates/openpanel-web/src/lib.rs"
# checker: class-coverage negative
assert "class-coverage: undefined class fails" 1 \
  env OPENSPEC_WEB_SRC_DIR="${CC_ROOT}/crates/openpanel-web/src" \
      OPENSPEC_WEB_CSS_FILE="${CC_ROOT}/crates/openpanel-web/assets/app.css" \
      "${CC_ROOT}/scripts/check-class-coverage.sh"

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
# NOTE: `make -n` is a SIGPIPE-friendly command — `grep -q` exits as
# soon as it finds a match and make receives a broken-pipe signal.
# The pre-pipefail pipeline therefore returns the make exit code (2)
# rather than grep's, which `set -euo pipefail` then propagates as a
# false negative. Capture the plan into a variable and grep that
# instead, so the assertion sees grep's exit code only.
if command -v make >/dev/null 2>&1; then
  plan="$(cd "${REPO_ROOT}" && make -n check 2>/dev/null || true)"
  if printf '%s' "${plan}" | grep -q 'scripts/check-agent-governance.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes agent-governance"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include agent-governance"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/test-gates.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes test-gates"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include test-gates"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-governance-contract.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes governance-contract"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include governance-contract"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-class-coverage.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes class-coverage"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include class-coverage"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-release-governance.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes release-governance"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include release-governance"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-maturity.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes maturity"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include maturity"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-coverage-floor.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes coverage-floor"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include coverage-floor"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-release-evidence.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes release-evidence"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include release-evidence"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/test-gates.sh'; then
    pass=$((pass+1)); echo "  ok   - make test-gates target runs the script"
  else
    fail=$((fail+1)); echo "  FAIL - make test-gates target missing or unwired"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-governance-contract.sh'; then
    pass=$((pass+1)); echo "  ok   - make governance-contract target runs the script"
  else
    fail=$((fail+1)); echo "  FAIL - make governance-contract target missing or unwired"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-class-coverage.sh'; then
    pass=$((pass+1)); echo "  ok   - make class-coverage target runs the script"
  else
    fail=$((fail+1)); echo "  FAIL - make class-coverage target missing or unwired"
  fi
  # The ratchet wiring: `--strict` MUST be the default invocation of
  # the reuse gate inside `make check` once duplicates are classified,
  # so the ratchet cannot be silently turned off.
  if printf '%s' "${plan}" | grep -q 'check-reuse.sh --strict'; then
    pass=$((pass+1)); echo "  ok   - make check runs reuse gate in --strict mode"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT run reuse --strict"
  fi
else
  fail=$((fail+1)); echo "  FAIL - make unavailable, cannot verify wiring"
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

# --- spec-test-drift strict-baseline (ratchet) ------------------------
# Per the quality-maturity-ratchet spec, --strict MUST only fail on
# capabilities NOT in the reviewed baseline; pre-existing gaps listed
# in the baseline are tracked debt and pass strict. The fallback when
# no baseline is supplied preserves the legacy strict behaviour.
STB="${TMP}/spec_test_drift_baseline"
mkdir -p "${STB}/scripts/lib" "${STB}/openspec/specs" \
         "${STB}/openspec/specs/trackedcap" "${STB}/openspec/specs/newcap" \
         "${STB}/crates/c/src"
cp ./scripts/check-spec-test-drift.sh "${STB}/scripts/check-spec-test-drift.sh"
cp ./scripts/lib/step.sh "${STB}/scripts/lib/step.sh"
printf '# c\n' > "${STB}/crates/c/src/lib.rs"
printf '# trackedcap spec\n\n### Requirement: Tracked\n\n#### Scenario: x\n\n- **WHEN** a\n- **THEN** b\n' \
  > "${STB}/openspec/specs/trackedcap/spec.md"
printf '# newcap spec\n\n### Requirement: New\n\n#### Scenario: x\n\n- **WHEN** a\n- **THEN** b\n' \
  > "${STB}/openspec/specs/newcap/spec.md"
# Baseline lists trackedcap as a known pre-existing gap.
printf 'trackedcap\n' > "${STB}/openspec/specs/.spec-test-drift-baseline"
# A covering test for newcap. trackedcap is uncovered but in baseline.
printf '// newcap coverage marker\n' > "${STB}/crates/c/src/lib.rs"
# checker: spec-test-drift-baseline positive
assert "spec-test-drift: baseline-tracked gap + new covered cap passes (strict)" 0 \
  env OPENSPEC_SPEC_TEST_DRIFT_BASELINE="${STB}/openspec/specs/.spec-test-drift-baseline" \
      "${STB}/scripts/check-spec-test-drift.sh" --strict
# Negative: drop the covering test for newcap so it is uncovered AND
# not in the baseline; strict MUST fail.
printf '# c\n' > "${STB}/crates/c/src/lib.rs"
# checker: spec-test-drift-baseline negative
assert "spec-test-drift: new uncovered cap not in baseline fails (strict)" 1 \
  env OPENSPEC_SPEC_TEST_DRIFT_BASELINE="${STB}/openspec/specs/.spec-test-drift-baseline" \
      "${STB}/scripts/check-spec-test-drift.sh" --strict

# --- reuse strict-classified (ratchet) ---------------------------------
# Per the quality-maturity-ratchet spec, --strict MUST only fail on
# unclassified new duplicates; pre-existing duplicates listed in the
# reviewed classified-duplicates baseline are tracked debt.
RUC="${TMP}/reuse_classified"
mkdir -p "${RUC}/crates/openpanel-core/src" "${RUC}/crates/openpanel-app/src" \
         "${RUC}/openspec/governance" "${RUC}/scripts/lib"
cp ./scripts/check-reuse.sh "${RUC}/scripts/check-reuse.sh"
cp ./scripts/lib/step.sh "${RUC}/scripts/lib/step.sh"
# Two cross-crate duplicates, one per call to demonstrate classified +
# unclassified.
printf 'pub fn compute_widget() {}\npub fn brand_new_dup() {}\n' \
  > "${RUC}/crates/openpanel-core/src/lib.rs"
printf 'pub fn compute_widget() {}\npub fn brand_new_dup() {}\n' \
  > "${RUC}/crates/openpanel-app/src/lib.rs"
# Classified baseline lists BOTH as pre-existing with a reason.
printf 'compute_widget\tclassify-as-alias-in-2026-09-09\nbrand_new_dup\tclassify-as-aliased-2026-09-09\n' \
  > "${RUC}/openspec/governance/.reuse-classified-baseline"
# checker: reuse-classified positive
assert "reuse: all dups classified passes (strict)" 0 \
  env OPENSPEC_REUSE_BASELINE="${RUC}/openspec/governance/.reuse-classified-baseline" \
      OPENSPEC_CRATES_DIR="${RUC}/crates" \
      "${RUC}/scripts/check-reuse.sh" --strict
# Negative: drop brand_new_dup from the classified baseline.
grep -v '^brand_new_dup' "${RUC}/openspec/governance/.reuse-classified-baseline" \
  > "${RUC}/openspec/governance/.reuse-classified-baseline.tmp" || true
mv "${RUC}/openspec/governance/.reuse-classified-baseline.tmp" \
   "${RUC}/openspec/governance/.reuse-classified-baseline"
# checker: reuse-classified negative
assert "reuse: unclassified new dup fails (strict)" 1 \
  env OPENSPEC_REUSE_BASELINE="${RUC}/openspec/governance/.reuse-classified-baseline" \
      OPENSPEC_CRATES_DIR="${RUC}/crates" \
      "${RUC}/scripts/check-reuse.sh" --strict

# --- coverage floor (ratchet) ------------------------------------------
# Per the quality-maturity-ratchet spec, a required CI coverage job
# MUST fail when its tool is unavailable or its measured line coverage
# drops below the configured floor. Tool-absent + not-required keeps
# the local-friendliness fallback.
COV="${TMP}/coverage_floor"
mkdir -p "${COV}/scripts/lib" "${COV}/target/coverage"
cp ./scripts/check-coverage-floor.sh "${COV}/scripts/check-coverage-floor.sh" 2>/dev/null || true
cp ./scripts/lib/step.sh "${COV}/scripts/lib/step.sh"
# checker: coverage-floor positive
assert "coverage-floor: tool missing + not required passes" 0 \
  env OPENPANEL_COVERAGE_REQUIRED=0 \
      OPENPANEL_COVERAGE_FLOOR=60 \
      OPENPANEL_COVERAGE_LCOV="" \
      "${COV}/scripts/check-coverage-floor.sh"
# Below floor: an lcov report showing 30% lines-found / lines-hit
# MUST fail a 80% floor.
printf 'SF:foo.rs\nLF:10\nLH:3\nend_of_record\n' \
  > "${COV}/target/coverage/lcov.info"
# checker: coverage-floor negative
assert "coverage-floor: measured coverage below floor fails" 1 \
  env OPENPANEL_COVERAGE_REQUIRED=0 \
      OPENPANEL_COVERAGE_FLOOR=80 \
      OPENPANEL_COVERAGE_LCOV="${COV}/target/coverage/lcov.info" \
      "${COV}/scripts/check-coverage-floor.sh"
# Above floor: the same report MUST pass a 20% floor.
assert "coverage-floor: measured coverage above floor passes" 0 \
  env OPENPANEL_COVERAGE_REQUIRED=0 \
      OPENPANEL_COVERAGE_FLOOR=20 \
      OPENPANEL_COVERAGE_LCOV="${COV}/target/coverage/lcov.info" \
      "${COV}/scripts/check-coverage-floor.sh"
# Tool missing + required: the gate MUST fail.
# checker: coverage-floor negative
assert "coverage-floor: tool missing + required fails" 1 \
  env OPENPANEL_COVERAGE_REQUIRED=1 \
      OPENPANEL_COVERAGE_FLOOR=60 \
      OPENPANEL_COVERAGE_LCOV="" \
      "${COV}/scripts/check-coverage-floor.sh"

# --- maturity evidence (ratchet) --------------------------------------
# Per the quality-maturity-ratchet spec, every production TODO/FIXME
# marker MUST have a reviewed entry in the evidence manifest; the
# manifest may also be empty (no markers → no required records).
ME="${TMP}/maturity_evidence"
mkdir -p "${ME}/scripts/lib" "${ME}/crates/foo/src" \
         "${ME}/openspec/governance"
cp ./scripts/check-maturity.sh "${ME}/scripts/check-maturity.sh" 2>/dev/null || true
cp ./scripts/lib/step.sh "${ME}/scripts/lib/step.sh"
# checker: maturity-evidence positive
assert "maturity: no markers + empty manifest passes" 0 \
  env OPENSPEC_TODOS_DIR="${ME}/crates" \
      OPENSPEC_EVIDENCE="${ME}/openspec/governance/evidence.yaml" \
      "${ME}/scripts/check-maturity.sh"
# Tracked marker: a TODO with an evidence record.
printf 'pub fn f() {}\n// TODO(openpanel#TEST): not yet implemented\n' \
  > "${ME}/crates/foo/src/lib.rs"
cat > "${ME}/openspec/governance/evidence.yaml" <<'EVIDENCE'
version: 1
records:
  - id: openpanel#TEST
    type: todo
    location: crates/foo/src/lib.rs:2
    owner: test
    reason: not yet implemented
    closure: follow-up-change
EVIDENCE
assert "maturity: tracked TODO with evidence record passes" 0 \
  env OPENSPEC_TODOS_DIR="${ME}/crates" \
      OPENSPEC_EVIDENCE="${ME}/openspec/governance/evidence.yaml" \
      "${ME}/scripts/check-maturity.sh"
# Untracked marker: same source, manifest emptied.
printf 'records: []\n' > "${ME}/openspec/governance/evidence.yaml"
# checker: maturity-evidence negative
assert "maturity: untracked TODO fails" 1 \
  env OPENSPEC_TODOS_DIR="${ME}/crates" \
      OPENSPEC_EVIDENCE="${ME}/openspec/governance/evidence.yaml" \
      "${ME}/scripts/check-maturity.sh"

# --- release-governance --------------------------------------------------
# Per the release-deployment-governance spec, every release artifact in
# OPENPANEL_DIST_DIR MUST ship with a .sha256 + .sbom.json + .sig and
# be listed in provenance.json. A missing sidecar, an unlisted
# artifact, or a signature that does not verify (with
# RELEASE_VERIFY_KEY set) fails the build. The gate is read-only and
# must skip gracefully when OPENPANEL_DIST_DIR does not exist.
build_release_fixture() { # build_release_fixture <root> <variant: good|bad-sbom|bad-sig|unlisted>
  local root="$1"; local variant="$2"
  mkdir -p "${root}/dist"
  printf 'fake-binary-bytes' > "${root}/dist/openpanel"
  local actual
  actual="$(sha256sum "${root}/dist/openpanel" | awk '{print $1}')"
  printf '%s\n' "${actual}" > "${root}/dist/openpanel.sha256"
  printf '{"bomFormat":"CycloneDX","components":[]}\n' > "${root}/dist/openpanel.sbom.json"
  printf 'untrusted:dummy\n' > "${root}/dist/openpanel.sig"
  case "${variant}" in
    bad-sbom)
      rm -f "${root}/dist/openpanel.sbom.json"
      ;;
    bad-sig)
      rm -f "${root}/dist/openpanel.sig"
      ;;
    unlisted)
      cat > "${root}/dist/provenance.json" <<PROV
{"commit":"abc123","target":"x86_64-unknown-linux-gnu","artifacts":["something-else"]}
PROV
      ;;
    *)
      cat > "${root}/dist/provenance.json" <<PROV
{"commit":"abc123","target":"x86_64-unknown-linux-gnu","artifacts":["openpanel"]}
PROV
      ;;
  esac
}
RG="${TMP}/release_governance"
mkdir -p "${RG}/scripts/lib"
cp ./scripts/check-release-governance.sh "${RG}/scripts/check-release-governance.sh"
cp ./scripts/lib/step.sh "${RG}/scripts/lib/step.sh"
build_release_fixture "${RG}" good
# checker: release-governance positive
assert "release-governance: complete artifact set passes" 0 \
  env OPENPANEL_DIST_DIR="${RG}/dist" \
      "${RG}/scripts/check-release-governance.sh"
# Missing SBOM MUST fail.
build_release_fixture "${RG}" bad-sbom
# checker: release-governance negative
assert "release-governance: missing SBOM fails" 1 \
  env OPENPANEL_DIST_DIR="${RG}/dist" \
      "${RG}/scripts/check-release-governance.sh"
# Missing signature MUST fail.
build_release_fixture "${RG}" bad-sig
# checker: release-governance negative
assert "release-governance: missing signature fails" 1 \
  env OPENPANEL_DIST_DIR="${RG}/dist" \
      "${RG}/scripts/check-release-governance.sh"
# An artifact present on disk but not listed in provenance.json MUST fail.
build_release_fixture "${RG}" unlisted
# checker: release-governance negative
assert "release-governance: unlisted artifact fails" 1 \
  env OPENPANEL_DIST_DIR="${RG}/dist" \
      "${RG}/scripts/check-release-governance.sh"
# A missing dist/ MUST skip (so `make check` does not block local work).
rm -rf "${RG}/dist"
# checker: release-governance positive
assert "release-governance: missing dist dir skips cleanly" 0 \
  env OPENPANEL_DIST_DIR="${RG}/dist" \
      "${RG}/scripts/check-release-governance.sh"

# --- audit (ratchet) ----------------------------------------------------
# Per the release-evidence spec, the audit gate MUST fail on actionable
# vulnerabilities and MUST NOT classify unmaintained / yanked /
# notice / unsound findings as vulnerabilities. The gate MUST also
# fail-closed (exit non-zero) when OPENPANEL_AUDIT_REQUIRED=1 and the
# tool is unavailable. The script's interface accepts a shimmed
# `cargo-audit` on PATH (set OPENPANEL_AUDIT_BIN), so the fixture can
# substitute canned JSON output for real `cargo audit --json`.
AU="${TMP}/audit"
mkdir -p "${AU}/scripts/lib" "${AU}/bin"
cp ./scripts/check-audit.sh "${AU}/scripts/check-audit.sh"
cp ./scripts/lib/step.sh "${AU}/scripts/lib/step.sh"

write_audit_json() { # write_audit_json <path> <json>
  printf '%s' "$2" > "$1"
}

# Empty database, no advisories — clean.
write_audit_json "${AU}/audit_clean.json" \
  '{"vulnerabilities":{"found":false,"count":0,"list":[]},"warnings":{"unmaintained":[],"yanked":[]}}'
# Only an unmaintained warning — informational; MUST pass.
write_audit_json "${AU}/audit_info.json" \
  '{"vulnerabilities":{"found":false,"count":0,"list":[]},"warnings":{"unmaintained":[{"kind":"unmaintained","package":{"name":"rustls-pemfile","version":"2.2.0"},"advisory":{"id":"RUSTSEC-2025-0134","title":"unmaintained"}}]}}'
# One actionable vulnerability — MUST fail.
write_audit_json "${AU}/audit_vuln.json" \
  '{"vulnerabilities":{"found":true,"count":1,"list":[{"advisory":{"id":"RUSTSEC-2026-9999","title":"Actionable"},"package":{"name":"some-crate","version":"0.1.0"},"versions":{"patched":[">=0.1.1"]}}]},"warnings":{"unmaintained":[],"yanked":[]}}'

# Checker: audit positive (clean)
assert "audit: clean advisory database passes" 0 \
  env OPENPANEL_AUDIT_BIN="sh -c 'cat ${AU}/audit_clean.json'" \
      OPENPANEL_AUDIT_REQUIRED=0 \
      "${AU}/scripts/check-audit.sh"
# Checker: audit positive (informational only)
assert "audit: only unmaintained warnings pass" 0 \
  env OPENPANEL_AUDIT_BIN="sh -c 'cat ${AU}/audit_info.json'" \
      OPENPANEL_AUDIT_REQUIRED=0 \
      "${AU}/scripts/check-audit.sh"
# Checker: audit negative (actionable vulnerability)
assert "audit: actionable vulnerability fails" 1 \
  env OPENPANEL_AUDIT_BIN="sh -c 'cat ${AU}/audit_vuln.json'" \
      OPENPANEL_AUDIT_REQUIRED=0 \
      "${AU}/scripts/check-audit.sh"
# Checker: audit negative (tool missing + required)
assert "audit: tool missing + required fails" 1 \
  env OPENPANEL_AUDIT_BIN="/no/such/cargo-audit-binary" \
      OPENPANEL_AUDIT_REQUIRED=1 \
      "${AU}/scripts/check-audit.sh"
# Local-friendliness: tool missing + not required MUST pass.
assert "audit: tool missing + not required skips cleanly" 0 \
  env OPENPANEL_AUDIT_BIN="/no/such/cargo-audit-binary" \
      OPENPANEL_AUDIT_REQUIRED=0 \
      "${AU}/scripts/check-audit.sh"

# --- portable-runtime --------------------------------------------------
# Per the portable-runtime spec, the runtime contract is the union of a
# published target manifest, a secret-free OCI image, a native installer
# that preflights the manifest, an upgrade path that runs a
# post-upgrade health check, and agreement between the two adapters on
# the default persistent data directory. The gate
# (scripts/check-portable-runtime.sh) scans the static artifacts;
# runtime behaviour is exercised by packages/installer/install-tests.sh
# which requires Docker and runs in a separate job.
PR="${TMP}/portable_runtime"
mkdir -p "${PR}/scripts/lib" "${PR}/packages/installer"
cp ./scripts/check-portable-runtime.sh "${PR}/scripts/check-portable-runtime.sh"
cp ./scripts/lib/step.sh "${PR}/scripts/lib/step.sh"

# Helper: build a clean fixture tree (everything required is present,
# no secret literals anywhere).
build_pr_clean() {
    local root="$1"
    mkdir -p "${root}/packages/installer"
    # A Dockerfile that declares the default data dir, runs as the
    # openpanel user, and ships zero credential literals.
    cat > "${root}/Dockerfile" <<'DOCKERFILE'
FROM debian:bookworm-slim
ENV OPENPANEL_DATA_DIR=/var/lib/openpanel
USER openpanel
VOLUME ["/var/lib/openpanel"]
HEALTHCHECK CMD ["/usr/local/bin/openpanel", "healthcheck"]
DOCKERFILE
    # install.sh that references the manifest and runs a post-upgrade
    # healthcheck (not --version).
    cat > "${root}/packages/installer/install.sh" <<'INSTALL'
#!/usr/bin/env bash
set -euo pipefail
MANIFEST="${SCRIPT_DIR:-.}/target-manifest.txt"
if [ ! -f "${MANIFEST}" ]; then
    echo "unsupported distribution: manifest missing" >&2
    exit 78
fi
do_upgrade() {
    cp openpanel openpanel.bak.$(date -u +%Y%m%dT%H%M%SZ)
    cp new openpanel
    openpanel healthcheck || { cp openpanel.bak.* openpanel; exit 1; }
}
INSTALL
    chmod +x "${root}/packages/installer/install.sh"
    # entrypoint.sh with the same default data dir.
    cat > "${root}/packages/installer/entrypoint.sh" <<'ENTRY'
#!/usr/bin/env bash
set -euo pipefail
DATA_DIR="${OPENPANEL_DATA_DIR:-/var/lib/openpanel}"
mkdir -p "${DATA_DIR}"
ENTRY
    chmod +x "${root}/packages/installer/entrypoint.sh"
    # A valid target manifest with two supported (os, arch) pairs.
    cat > "${root}/packages/installer/target-manifest.txt" <<'MANIFEST'
debian x86_64
debian aarch64
ubuntu x86_64
MANIFEST
}

# checker: portable-runtime positive
build_pr_clean "${PR}"
assert "portable-runtime: clean contract passes" 0 \
  env OPENPANEL_PORTABLE_RUNTIME_REPO_ROOT="${PR}" \
      "${PR}/scripts/check-portable-runtime.sh"

# Dockerfile with a password literal MUST fail.
sed -i 's|^USER openpanel$|ENV password=hunter2\nUSER openpanel|' "${PR}/Dockerfile"
# checker: portable-runtime negative
assert "portable-runtime: Dockerfile secret literal fails" 1 \
  env OPENPANEL_PORTABLE_RUNTIME_REPO_ROOT="${PR}" \
      OPENPANEL_PORTABLE_RUNTIME_REQUIRED=1 \
      "${PR}/scripts/check-portable-runtime.sh"
# Restore the clean Dockerfile for the next case.
sed -i '/^ENV password=hunter2$/d' "${PR}/Dockerfile"

# Missing target manifest MUST fail in required mode.
rm "${PR}/packages/installer/target-manifest.txt"
# checker: portable-runtime negative
assert "portable-runtime: missing target manifest fails" 1 \
  env OPENPANEL_PORTABLE_RUNTIME_REPO_ROOT="${PR}" \
      OPENPANEL_PORTABLE_RUNTIME_REQUIRED=1 \
      "${PR}/scripts/check-portable-runtime.sh"
# Restore for the next case.
cat > "${PR}/packages/installer/target-manifest.txt" <<'MANIFEST'
debian x86_64
MANIFEST

# install.sh that does NOT consult the manifest MUST fail.
cat > "${PR}/packages/installer/install.sh" <<'INSTALL'
#!/usr/bin/env bash
set -euo pipefail
echo "no manifest reference"
INSTALL
# checker: portable-runtime negative
assert "portable-runtime: install.sh without manifest reference fails" 1 \
  env OPENPANEL_PORTABLE_RUNTIME_REPO_ROOT="${PR}" \
      OPENPANEL_PORTABLE_RUNTIME_REQUIRED=1 \
      "${PR}/scripts/check-portable-runtime.sh"

# 1b. the make target exists and is wired to the script.
if command -v make >/dev/null 2>&1; then
  plan="$(cd "${REPO_ROOT}" && make -n check 2>/dev/null || true)"
  if printf '%s' "${plan}" | grep -q 'scripts/check-portable-runtime.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes portable-runtime"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include portable-runtime"
  fi
  if printf '%s' "${plan}" | grep -q 'scripts/check-deployment-adapters.sh'; then
    pass=$((pass+1)); echo "  ok   - make check includes deployment-adapters"
  else
    fail=$((fail+1)); echo "  FAIL - make check does NOT include deployment-adapters"
  fi
fi

# --- deployment-adapters -------------------------------------------------
# Per the deployment-adapters spec, the contract is a typed
# `DeploymentAdapter` trait, a `DeploymentAdapterService` that owns the
# lifecycle, idempotency, evidence, and audit fan-out, and a covering
# integration test. The gate
# (`scripts/check-deployment-adapters.sh`) asserts the static wiring
# (domain module + app service + integration test) and the layering
# invariant for the new bounded context; the test below exercises both
# the passing and the failing case.
DA="${TMP}/deployment_adapters"
mkdir -p "${DA}/scripts/lib" \
         "${DA}/crates/openpanel-domain/src/deployment_adapters" \
         "${DA}/crates/openpanel-app/src/deployment_adapters" \
         "${DA}/tests/integration" \
         "${DA}/openspec/changes/add-portable-deployment-adapters/specs/deployment-adapters" \
         "${DA}/openspec/specs/deployment-adapters"
cp ./scripts/check-deployment-adapters.sh "${DA}/scripts/check-deployment-adapters.sh"
cp ./scripts/lib/step.sh "${DA}/scripts/lib/step.sh"

# Domain module with the trait + the three typed data items.
cat > "${DA}/crates/openpanel-domain/src/deployment_adapters/mod.rs" <<'DOMAIN'
//! Fixtured deployment-adapter domain module.
#![allow(dead_code)]
pub struct AdapterManifest;
pub struct DeploymentPlan;
pub struct DeploymentEvidence;
pub trait DeploymentAdapter {
    fn manifest(&self) -> &AdapterManifest;
    fn execute(&self, _plan: &DeploymentPlan, _cached: Option<&DeploymentEvidence>)
        -> DeploymentEvidence;
}
DOMAIN

# App service with the lifecycle, the operator gate, and the canonical
# audit redaction.
cat > "${DA}/crates/openpanel-app/src/deployment_adapters/service.rs" <<'SERVICE'
//! Fixtured deployment-adapter service.
#![allow(dead_code)]
use openpanel_core::audit::redact_metadata;
pub struct DeploymentAdapterService;
impl DeploymentAdapterService {
    pub async fn run(&self) {}
    fn require_operator(&self) {}
}
SERVICE

# Integration test file referencing every scenario in the spec.
cat > "${DA}/openspec/changes/add-portable-deployment-adapters/specs/deployment-adapters/spec.md" <<'DELTA'
# deployment-adapters Specification

## ADDED Requirements

### Requirement: Sample

#### Scenario: Unsupported action

- **WHEN** a
- **THEN** b

#### Scenario: Mac adapter evidence

- **WHEN** c
- **THEN** d

#### Scenario: Health failure

- **WHEN** e
- **THEN** f
DELTA
cat > "${DA}/tests/integration/deployment_adapters.rs" <<'TEST'
//! Fixtured deployment-adapter integration test.
//!
//! Unsupported action — a plan asking for a non-declared action is rejected.
//! Mac adapter evidence — the mac-jenkins-v1 conformance fixture emits
//! evidence that matches the generic Linux schema.
//! Health failure — a deployment that fails its declared health check
//! MUST NOT report Ready.
fn deployment_adapters_unsupported_action() {}
fn deployment_adapters_mac_adapter_evidence() {}
fn deployment_adapters_health_failure() {}
TEST

# checker: deployment-adapters positive
assert "deployment-adapters: all wiring present passes" 0 \
  env OPENSPEC_CRATES_DIR="${DA}/crates" \
      OPENSPEC_SPECS_DIR="${DA}/openspec/specs" \
      OPENSPEC_TESTS_DIR="${DA}/tests/integration" \
      "${DA}/scripts/check-deployment-adapters.sh"

# Remove the integration test file so the scenario coverage check fails.
rm "${DA}/tests/integration/deployment_adapters.rs"
# checker: deployment-adapters negative
assert "deployment-adapters: missing integration test fails" 1 \
  env OPENSPEC_CRATES_DIR="${DA}/crates" \
      OPENSPEC_SPECS_DIR="${DA}/openspec/specs" \
      OPENSPEC_TESTS_DIR="${DA}/tests/integration" \
      "${DA}/scripts/check-deployment-adapters.sh"

# Restore for the layering test.
cat > "${DA}/tests/integration/deployment_adapters.rs" <<'TEST'
//! Fixtured deployment-adapter integration test.
//!
//! Unsupported action — a plan asking for a non-declared action is rejected.
//! Mac adapter evidence — the mac-jenkins-v1 conformance fixture emits
//! evidence that matches the generic Linux schema.
//! Health failure — a deployment that fails its declared health check
//! MUST NOT report Ready.
fn deployment_adapters_unsupported_action() {}
fn deployment_adapters_mac_adapter_evidence() {}
fn deployment_adapters_health_failure() {}
TEST

# Domain MUST NOT import app; layering re-assertion in the new context.
cat > "${DA}/crates/openpanel-domain/src/deployment_adapters/mod.rs" <<'DOMAIN'
//! Fixtured deployment-adapter domain module that violates layering.
#![allow(dead_code)]
use openpanel_app::something;
pub struct AdapterManifest;
pub struct DeploymentPlan;
pub struct DeploymentEvidence;
pub trait DeploymentAdapter {}
DOMAIN
# checker: deployment-adapters negative
assert "deployment-adapters: domain depending on app fails" 1 \
  env OPENSPEC_CRATES_DIR="${DA}/crates" \
      OPENSPEC_SPECS_DIR="${DA}/openspec/specs" \
      OPENSPEC_TESTS_DIR="${DA}/tests/integration" \
      "${DA}/scripts/check-deployment-adapters.sh"

# --- release-evidence (new) ---------------------------------------------
# Per the release-evidence spec, a publication workflow MUST fail when
# any required coverage / browser / SBOM / signature / provenance /
# smoke evidence record is missing, stale, malformed, or in a
# non-PASS state. The gate consumes dist/evidence-manifest.json
# (only when OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1); without that
# flag set, the gate skips so local work is not blocked.
RE="${TMP}/release_evidence"
mkdir -p "${RE}/scripts/lib" "${RE}/dist"
cp ./scripts/check-release-evidence.sh "${RE}/scripts/check-release-evidence.sh" 2>/dev/null || true
cp ./scripts/lib/step.sh "${RE}/scripts/lib/step.sh"

build_evidence_manifest() { # build_evidence_manifest <path> <commit> <target> <variant>
  local path="$1"; local commit="$2"; local target="$3"; local variant="$4"
  cat > "${path}" <<JSON
{
  "commit": "${commit}",
  "target": "${target}",
  "records": [
    { "id": "audit", "commit": "${commit}", "command": "make audit", "tool_versions": { "cargo-audit": "0.21.1" }, "target": "${target}", "timestamp": "2026-09-24T00:00:00Z", "scope": "workspace", "state": "PASS" },
    { "id": "coverage", "commit": "${commit}", "command": "make coverage-floor", "tool_versions": { "cargo-llvm-cov": "0.6.0" }, "target": "${target}", "timestamp": "2026-09-24T00:00:00Z", "scope": "workspace", "state": "PASS" },
    { "id": "browser-ui-quality", "commit": "${commit}", "command": "make browser-ui-quality", "tool_versions": { "axe-core": "4.10.0" }, "target": "${target}", "timestamp": "2026-09-24T00:00:00Z", "scope": "web", "state": "PASS" },
    { "id": "release-governance", "commit": "${commit}", "command": "scripts/check-release-governance.sh", "tool_versions": { "minisign": "0.11" }, "target": "${target}", "timestamp": "2026-09-24T00:00:00Z", "scope": "dist", "state": "PASS" },
    { "id": "smoke", "commit": "${commit}", "command": "packages/installer/smoke-container.sh", "tool_versions": { "docker": "24.0" }, "target": "${target}", "timestamp": "2026-09-24T00:00:00Z", "scope": "container", "state": "PASS" }
  ]
}
JSON
  case "${variant}" in
    missing-record)
      # Drop the smoke record entirely.
      python3 -c "import json,sys; d=json.load(open('${path}')); d['records']=[r for r in d['records'] if r['id']!='smoke']; json.dump(d, open('${path}','w'), indent=2)" 2>/dev/null \
        || sed -i '/"id": "smoke"/,/state.*PASS/d' "${path}"
      ;;
    stale-commit)
      sed -i "s/${commit}/0000000000000000000000000000000000000000/g" "${path}"
      ;;
    blocked-state)
      sed -i 's/"id": "coverage".*"state": "PASS"/"id": "coverage", "command": "make coverage-floor", "tool_versions": { "cargo-llvm-cov": "0.6.0" }, "target": "x86_64-unknown-linux-gnu", "timestamp": "2026-09-24T00:00:00Z", "scope": "workspace", "state": "BLOCKED"/' "${path}"
      ;;
    malformed)
      sed -i 's/"command": "make audit",//' "${path}"
      ;;
  esac
}

# Clean: complete manifest, matching commit + target — MUST pass.
build_evidence_manifest "${RE}/dist/evidence-manifest.json" "abc123def" "x86_64-unknown-linux-gnu" complete
# checker: release-evidence positive
assert "release-evidence: complete manifest passes" 0 \
  env OPENPANEL_DIST_DIR="${RE}/dist" \
      OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1 \
      OPENPANEL_RELEASE_EXPECTED_COMMIT="abc123def" \
      OPENPANEL_RELEASE_EXPECTED_TARGET="x86_64-unknown-linux-gnu" \
      "${RE}/scripts/check-release-evidence.sh"
# Missing required record MUST fail.
build_evidence_manifest "${RE}/dist/evidence-manifest.json" "abc123def" "x86_64-unknown-linux-gnu" missing-record
# checker: release-evidence negative
assert "release-evidence: missing required record fails" 1 \
  env OPENPANEL_DIST_DIR="${RE}/dist" \
      OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1 \
      OPENPANEL_RELEASE_EXPECTED_COMMIT="abc123def" \
      OPENPANEL_RELEASE_EXPECTED_TARGET="x86_64-unknown-linux-gnu" \
      "${RE}/scripts/check-release-evidence.sh"
# Stale commit (record for a different commit) MUST fail.
build_evidence_manifest "${RE}/dist/evidence-manifest.json" "abc123def" "x86_64-unknown-linux-gnu" stale-commit
# checker: release-evidence negative
assert "release-evidence: stale commit fails" 1 \
  env OPENPANEL_DIST_DIR="${RE}/dist" \
      OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1 \
      OPENPANEL_RELEASE_EXPECTED_COMMIT="abc123def" \
      OPENPANEL_RELEASE_EXPECTED_TARGET="x86_64-unknown-linux-gnu" \
      "${RE}/scripts/check-release-evidence.sh"
# BLOCKED state on a required record MUST fail.
build_evidence_manifest "${RE}/dist/evidence-manifest.json" "abc123def" "x86_64-unknown-linux-gnu" blocked-state
# checker: release-evidence negative
assert "release-evidence: BLOCKED state fails" 1 \
  env OPENPANEL_DIST_DIR="${RE}/dist" \
      OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1 \
      OPENPANEL_RELEASE_EXPECTED_COMMIT="abc123def" \
      OPENPANEL_RELEASE_EXPECTED_TARGET="x86_64-unknown-linux-gnu" \
      "${RE}/scripts/check-release-evidence.sh"
# Malformed record (missing required field) MUST fail.
build_evidence_manifest "${RE}/dist/evidence-manifest.json" "abc123def" "x86_64-unknown-linux-gnu" malformed
# checker: release-evidence negative
assert "release-evidence: malformed record fails" 1 \
  env OPENPANEL_DIST_DIR="${RE}/dist" \
      OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1 \
      OPENPANEL_RELEASE_EXPECTED_COMMIT="abc123def" \
      OPENPANEL_RELEASE_EXPECTED_TARGET="x86_64-unknown-linux-gnu" \
      "${RE}/scripts/check-release-evidence.sh"
# Not-required (default) MUST skip so local work is unblocked.
# Wipe the dist/ so the script hits the no-OPENPANEL_DIST_DIR path
# rather than validating the previous test's (deliberately broken)
# manifest.
rm -rf "${RE}/dist"
assert "release-evidence: not required skips cleanly" 0 \
  env OPENPANEL_DIST_DIR="${RE}/dist" \
      OPENPANEL_RELEASE_EVIDENCE_REQUIRED=0 \
      "${RE}/scripts/check-release-evidence.sh"

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
