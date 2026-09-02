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
#   - spec-test-drift       (default mode reports, does not fail)
#   - spec-drift            (archive delta MUST exist in live spec)
#   - make check integration (test-gates target is wired into check)
#   - ci configuration      (make check + no continue-on-error + --strict)
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
TS_BAD="${TMP}/changes_bad"; mkdir -p "${TS_BAD}/c"
printf '# T\n\n## 2. Implementation\n- [ ] y\n\n## 1. Testing\n- [ ] x\n' > "${TS_BAD}/c/tasks.md"
assert "tasks: correctly ordered passes" 0 env OPENSPEC_CHANGES_DIR="${TS_GOOD}" ./scripts/check-tasks-testing-first.sh
assert "tasks: reordered fails" 1 env OPENSPEC_CHANGES_DIR="${TS_BAD}" ./scripts/check-tasks-testing-first.sh

# --- layering ------------------------------------------------------------
LAY="${TMP}/crates"
mkdir -p "${LAY}/openpanel-domain/src/x" "${LAY}/openpanel-app/src/y"
printf 'use openpanel_app::Foo;\n' > "${LAY}/openpanel-domain/src/x/mod.rs"
printf 'pub fn ok() {}\n' > "${LAY}/openpanel-app/src/y/mod.rs"
assert "layering: domain->app import fails" 1 env OPENSPEC_CRATES_DIR="${LAY}" ./scripts/check-layering.sh
rm "${LAY}/openpanel-domain/src/x/mod.rs"
printf 'pub fn ok() {}\n' > "${LAY}/openpanel-domain/src/x/mod.rs"
assert "layering: clean tree passes" 0 env OPENSPEC_CRATES_DIR="${LAY}" ./scripts/check-layering.sh

# --- reuse ---------------------------------------------------------------
RU="${TMP}/crates2"
mkdir -p "${RU}/openpanel-core/src" "${RU}/openpanel-app/src"
printf 'pub fn compute_widget() {}\n' > "${RU}/openpanel-core/src/lib.rs"
printf 'pub fn compute_widget() {}\n' > "${RU}/openpanel-app/src/lib.rs"
assert "reuse: cross-crate dup fn fails (strict)" 1 env OPENSPEC_CRATES_DIR="${RU}" ./scripts/check-reuse.sh --strict
assert "reuse: cross-crate dup fn warns (default)" 0 env OPENSPEC_CRATES_DIR="${RU}" ./scripts/check-reuse.sh
printf 'pub fn compute_widget() {}\n' > "${RU}/openpanel-core/src/lib.rs"
printf 'pub fn other_unique() {}\n' > "${RU}/openpanel-app/src/lib.rs"
assert "reuse: unique names pass" 0 env OPENSPEC_CRATES_DIR="${RU}" ./scripts/check-reuse.sh

# --- spec-test-drift (default = warn, exit 0) -----------------------------
assert "spec-test-drift: default mode never fails" 0 ./scripts/check-spec-test-drift.sh

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
assert "spec-drift: clean archive/live passes" 0 "${SD_ROOT}/scripts/check-spec-drift.sh"

# Negative: live spec is missing the requirement the delta introduces.
printf '# example spec\n' > "${SD_ROOT}/openspec/specs/example/spec.md"
assert "spec-drift: missing requirement fails" 1 "${SD_ROOT}/scripts/check-spec-drift.sh"

# Restore live spec for the rest of the suite.
printf '# example spec\n\n### Requirement: Fixture Requirement\n' \
  > "${SD_ROOT}/openspec/specs/example/spec.md"

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
  BAD_TS_DIR="${TMP}/propagation"
  mkdir -p "${BAD_TS_DIR}/c"
  printf '# T\n\n## 2. Implementation\n- [ ] y\n' > "${BAD_TS_DIR}/c/tasks.md"
  if env OPENSPEC_CHANGES_DIR="${BAD_TS_DIR}" TEST_GATES_REENTRY=1 \
       ./scripts/test-gates.sh >/dev/null 2>&1; then
    fail=$((fail+1)); echo "  FAIL - propagation: broken tasks fixture should yield non-zero"
  else
    pass=$((pass+1)); echo "  ok   - propagation: broken tasks fixture yields non-zero"
  fi
else
  # Inner call: just verify the chain exits non-zero on broken tasks.
  assert "propagation (reentry): broken tasks fails" 1 env OPENSPEC_CHANGES_DIR="${TMP}/reentry_broken" ./scripts/check-tasks-testing-first.sh || true
fi

echo ""
echo "gate self-tests: ${pass} passed, ${fail} failed"
[ "${fail}" -eq 0 ]
