#!/usr/bin/env bash
# scripts/check-tasks-testing-first.sh — enforce the TDD standing rule.
#
# Every active OpenSpec change's tasks.md MUST list the testing group
# (## 1. Testing) before any implementation group. This makes the
# long-standing "tests first" rule machine-enforced (closes the
# "SHOULD be extended ... in a follow-up change" TODO in the testing
# spec).
#
# Prints `step: tasks-testing-first status: ok | failed` and exits
# non-zero on the first offending change.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

CHANGES_DIR="${OPENSPEC_CHANGES_DIR:-openspec/changes}"
fail=0
offenders=""

if [ -d "${CHANGES_DIR}" ]; then
  for tasks in "${CHANGES_DIR}"/*/tasks.md; do
    [ -f "${tasks}" ] || continue
    # Find the line numbers of the first testing group and the first
    # non-testing implementation-style group.
    testing_line=$(grep -nE '^##[[:space:]]+[0-9]+[.[:space:]]*Testing' "${tasks}" | head -1 | cut -d: -f1 || true)
    impl_line=$(grep -nE '^##[[:space:]]+[0-9]+[.[:space:]]+(Implementation|Code|Apply)' "${tasks}" | head -1 | cut -d: -f1 || true)
    if [ -z "${testing_line}" ]; then
      fail=1; offenders="${offenders} $(basename "$(dirname "${tasks}")")"; continue
    fi
    if [ -n "${impl_line}" ] && [ "${impl_line}" -lt "${testing_line}" ]; then
      fail=1; offenders="${offenders} $(basename "$(dirname "${tasks}")")"; fi
  done
fi

if [ "${fail}" -eq 0 ]; then
  step "tasks-testing-first" true
else
  echo ""
  echo "step: tasks-testing-first status: failed"
  echo "  These changes must put '## 1. Testing' before any implementation group:"
  for o in ${offenders}; do echo "    - ${o}"; done
  exit 1
fi
