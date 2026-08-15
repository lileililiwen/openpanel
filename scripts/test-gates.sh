#!/usr/bin/env bash
# scripts/test-gates.sh — self-test for the agent-quality gates.
#
# Exercises the four bash gates against tiny fixtures so a regression in
# a gate's logic is caught without a full `make check`. Not wired into
# `make check` (which runs the real Rust gates) but runnable via
# `make test-gates`.
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

echo ""
echo "gate self-tests: ${pass} passed, ${fail} failed"
[ "${fail}" -eq 0 ]
