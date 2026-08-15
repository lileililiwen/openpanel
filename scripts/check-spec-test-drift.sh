#!/usr/bin/env bash
# scripts/check-spec-test-drift.sh — spec <-> test coverage gate.
#
# For every archived spec under openspec/specs/<cap>/spec.md, count its
# "#### Scenario:" entries and verify that at least one test in the
# workspace references the capability (by grepping the capability name
# in tests/ and #[cfg(test)] modules). This catches spec-vs-code drift:
# a requirement that exists on paper but has no executable proof.
#
# Mode:
#   default  — report gaps for PRE-EXISTING specs as warnings (exit 0).
#   --strict — specs created after this change MUST have a covering test;
#              gaps fail the build (exit 1).
#
# Degrades to SKIPPED when neither rg nor grep is available.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

if command -v rg >/dev/null 2>&1; then
  GREP=rg; GREP_FILES=(--files -g '*.rs')
elif command -v grep >/dev/null 2>&1; then
  GREP=grep
else
  echo ""
  echo "step: spec-test-drift status: skipped (no grep/rg)"
  exit 0
fi

specs_dir="openspec/specs"
[ -d "${specs_dir}" ] || { step "spec-test-drift" true; exit 0; }

# Gather test source files once.
if [ "${GREP}" = "rg" ]; then
  mapfile -t test_files < <(rg --files -g 'tests/**/*.rs' -g 'crates/**/*.rs' 2>/dev/null || true)
else
  test_files=($(grep -rlE '.' tests crates 2>/dev/null | grep '\.rs$' || true))
fi

report=""
strict_fail=0

for spec in "${specs_dir}"/*/spec.md; do
  [ -f "${spec}" ] || continue
  cap="$(basename "$(dirname "${spec}")")"
  scenarios=$(grep -cE '^#### Scenario:' "${spec}" 2>/dev/null || echo 0)
  [ "${scenarios}" -eq 0 ] && continue

  # Does any test file reference this capability name?
  covered=0
  for tf in "${test_files[@]:-}"; do
    [ -f "${tf}" ] || continue
    if grep -qE "\b${cap}\b" "${tf}" 2>/dev/null; then covered=1; break; fi
  done

  if [ "${covered}" -eq 0 ]; then
    if [ "${STRICT}" -eq 1 ]; then
      strict_fail=1
      report="${report}  - [FAIL] ${cap}: ${scenarios} scenario(s), no covering test\n"
    else
      report="${report}  - [warn] ${cap}: ${scenarios} scenario(s), no covering test\n"
    fi
  fi
done

if [ "${strict_fail}" -eq 1 ]; then
  echo ""
  echo "step: spec-test-drift status: failed"
  printf "%b" "${report}"
  exit 1
fi

if [ -n "${report}" ]; then
  echo ""
  echo "step: spec-test-drift status: ok (gaps reported as warnings)"
  printf "%b" "${report}"
else
  step "spec-test-drift" true
fi
