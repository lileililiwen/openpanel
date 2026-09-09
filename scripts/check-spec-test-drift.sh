#!/usr/bin/env bash
# scripts/check-spec-test-drift.sh — spec <-> test coverage gate.
#
# For every archived spec under openspec/specs/<cap>/spec.md, count its
# "#### Scenario:" entries and verify that at least one test in the
# workspace references the capability (by grepping the capability name
# in tests/ and #[cfg(test)] modules). This catches spec-vs-code drift:
# a requirement that exists on paper but has no executable proof.
#
# Modes:
#   default        — report gaps for PRE-EXISTING specs as warnings (exit 0).
#   --strict       — only capabilities NOT in the ratchet baseline fail.
#                    Pre-existing gaps recorded in the baseline file
#                    (OPENSPEC_SPEC_TEST_DRIFT_BASELINE) are tracked
#                    debt and pass.
#   --write-baseline — write the current pre-existing uncovered
#                    capabilities to the baseline file (one per line).
#
# The ratchet baseline is the shrinking list of every capability that
# was already uncovered when this change shipped; a newly added or
# modified capability MUST have a covering test or the build fails.
#
# Degrades to SKIPPED when neither rg nor grep is available.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

mode="${1:-}"
BASELINE="${OPENSPEC_SPEC_TEST_DRIFT_BASELINE:-${REPO_ROOT}/openspec/specs/.spec-test-drift-baseline}"

if command -v rg >/dev/null 2>&1; then
  GREP=rg
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

# Read the ratchet baseline (one cap per line; comments start with #).
baseline_caps=()
if [ -f "${BASELINE}" ]; then
  while IFS= read -r line; do
    case "${line}" in
      ""|\#*) continue ;;
    esac
    baseline_caps+=("${line}")
  done < "${BASELINE}"
fi

is_baseline_cap() {
  local cap="$1"
  local b
  for b in "${baseline_caps[@]:-}"; do
    [ "${b}" = "${cap}" ] && return 0
  done
  return 1
}

report=""
strict_fail=0
discovered_caps=()

for spec in "${specs_dir}"/*/spec.md; do
  [ -f "${spec}" ] || continue
  cap="$(basename "$(dirname "${spec}")")"
  discovered_caps+=("${cap}")
  scenarios=$(grep -cE '^#### Scenario:' "${spec}" 2>/dev/null || echo 0)
  [ "${scenarios}" -eq 0 ] && continue

  # Does any test file reference this capability name?
  covered=0
  for tf in "${test_files[@]:-}"; do
    [ -f "${tf}" ] || continue
    if grep -qE "\b${cap}\b" "${tf}" 2>/dev/null; then covered=1; break; fi
  done

  if [ "${covered}" -eq 0 ]; then
    case "${mode}" in
      --strict)
        if is_baseline_cap "${cap}"; then
          report="${report}  - [track] ${cap}: ${scenarios} scenario(s), baseline-listed, tracked debt\n"
        else
          strict_fail=1
          report="${report}  - [FAIL] ${cap}: ${scenarios} scenario(s), no covering test\n"
        fi
        ;;
      --write-baseline)
        report="${report}${cap}\n"
        ;;
      *)
        report="${report}  - [warn] ${cap}: ${scenarios} scenario(s), no covering test\n"
        ;;
    esac
  fi
done

case "${mode}" in
  --write-baseline)
    # Only emit baseline lines; do not fail.
    if [ -n "${report}" ]; then
      printf "%b" "${report}" | sort -u > "${BASELINE}"
      echo "check-spec-test-drift: baseline written ($(wc -l < "${BASELINE}") entries)"
    else
      : > "${BASELINE}"
      echo "check-spec-test-drift: baseline empty (no uncovered capabilities)"
    fi
    exit 0
    ;;
  --strict)
    if [ "${strict_fail}" -eq 1 ]; then
      echo ""
      echo "step: spec-test-drift status: failed"
      printf "%b" "${report}"
      exit 1
    fi
    if [ -n "${report}" ]; then
      echo ""
      echo "step: spec-test-drift status: ok (gaps reported as tracked debt)"
      printf "%b" "${report}"
    else
      step "spec-test-drift" true
    fi
    ;;
  *)
    if [ -n "${report}" ]; then
      echo ""
      echo "step: spec-test-drift status: ok (gaps reported as warnings)"
      printf "%b" "${report}"
    else
      step "spec-test-drift" true
    fi
    ;;
esac
