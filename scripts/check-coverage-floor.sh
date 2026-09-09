#!/usr/bin/env bash
# scripts/check-coverage-floor.sh — mandatory coverage floor gate.
#
# Per the quality-maturity-ratchet spec, a required CI coverage job
# MUST fail when its configured tool is unavailable or its measured
# line coverage drops below the configured floor. This gate is the
# strict, fail-on-miss counterpart of `make coverage` (which stays
# informational for local dev).
#
# Configuration (env vars; all optional):
#   OPENPANEL_COVERAGE_FLOOR    — required line-coverage percentage
#                                 (integer 1-100). Default 60.
#   OPENPANEL_COVERAGE_REQUIRED — set to 1 to fail when no coverage
#                                 tool is installed (the required CI
#                                 setting); set to 0 (default) to
#                                 behave like `make coverage` and
#                                 skip with a warning when the tool
#                                 is missing.
#   OPENPANEL_COVERAGE_LCOV     — path to an existing lcov.info file.
#                                 If set, the gate uses it directly
#                                 without re-running the tool (used
#                                 by the self-test). If unset, the
#                                 gate looks at
#                                 target/coverage/lcov.info (the
#                                 `make coverage` output path).
#
# The gate computes a single "line coverage" percentage from the
# LF/LH fields in the lcov report and compares it to the floor.
# Lcov format: each file has `SF:<path>` `LF:<lines found>`
# `LH:<lines hit>` records; the global totals are the sum of
# every LF/LH across all records.
#
# Exit codes:
#   0 — coverage ≥ floor (or skipped per OPENPANEL_COVERAGE_REQUIRED=0
#       when no tool + no report is present)
#   1 — coverage < floor, OR tool missing + required
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

FLOOR="${OPENPANEL_COVERAGE_FLOOR:-60}"
REQUIRED="${OPENPANEL_COVERAGE_REQUIRED:-0}"
LCOV="${OPENPANEL_COVERAGE_LCOV:-${REPO_ROOT}/target/coverage/lcov.info}"

# Sanity: the floor must be a positive integer in [1, 100]. An invalid
# value is reported and treated as the default (60) so a typo cannot
# silently make the gate permissive.
case "${FLOOR}" in
  ''|*[!0-9]*) FLOOR=60 ;;
esac
if [ "${FLOOR}" -lt 1 ] || [ "${FLOOR}" -gt 100 ]; then
  FLOOR=60
fi

# No lcov report yet — try to produce one with the available tool.
# We treat each tool as present only if its binary is on PATH.
has_tool=0
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  has_tool=1
elif command -v cargo-tarpaulin >/dev/null 2>&1; then
  has_tool=1
fi

if [ ! -f "${LCOV}" ] && [ "${has_tool}" -eq 1 ]; then
  mkdir -p "$(dirname "${LCOV}")"
  if command -v cargo-llvm-cov >/dev/null 2>&1; then
    cargo llvm-cov --workspace --all-targets --lcov --output-path "${LCOV}" >/dev/null 2>&1 || true
  else
    cargo tarpaulin --workspace --out Lcov --output-dir "$(dirname "${LCOV}")" >/dev/null 2>&1 || true
  fi
fi

# No report and no tool: behaviour is controlled by REQUIRED.
if [ ! -f "${LCOV}" ]; then
  if [ "${REQUIRED}" = "1" ]; then
    echo ""
    echo "step: coverage-floor status: failed (no coverage tool installed)"
    echo "  Install cargo-llvm-cov (recommended) or cargo-tarpaulin,"
    echo "  or run \`make coverage\` locally to populate ${LCOV}."
    exit 1
  fi
  echo ""
  echo "step: coverage-floor status: skipped (no coverage tool, OPENPANEL_COVERAGE_REQUIRED=0)"
  exit 0
fi

# Parse lcov: sum LF (lines found) and LH (lines hit) across all
# records. The script is deliberately minimal — the format is line-
# oriented, and we only need totals. Lines starting with `#` are
# comments and ignored.
total_lf=0
total_lh=0
while IFS= read -r line; do
  case "${line}" in
    LF:*)
      n="${line#LF:}"
      case "${n}" in ''|*[!0-9]*) ;; *) total_lf=$((total_lf + n)) ;; esac
      ;;
    LH:*)
      n="${line#LH:}"
      case "${n}" in ''|*[!0-9]*) ;; *) total_lh=$((total_lh + n)) ;; esac
      ;;
  esac
done < "${LCOV}"

if [ "${total_lf}" -eq 0 ]; then
  # An empty or non-numeric lcov (e.g. the stub emitted by
  # scripts/coverage.sh) has no measurable coverage.
  if [ "${REQUIRED}" = "1" ]; then
    echo ""
    echo "step: coverage-floor status: failed (lcov report is empty)"
    echo "  The configured tool did not produce any line records in ${LCOV}."
    exit 1
  fi
  echo ""
  echo "step: coverage-floor status: skipped (empty lcov report)"
  exit 0
fi

# Integer percentage: floor((LH * 100) / LF). The floor is what we
# compare against; a measured 59% on a 60% floor is a fail.
pct=$(( total_lh * 100 / total_lf ))

if [ "${pct}" -lt "${FLOOR}" ]; then
  echo ""
  echo "step: coverage-floor status: failed (${pct}% < ${FLOOR}% floor)"
  echo "  coverage: ${pct}% (${total_lh}/${total_lf} lines)"
  echo "  floor:    ${FLOOR}%"
  exit 1
fi

echo ""
echo "step: coverage-floor status: ok (${pct}% >= ${FLOOR}% floor)"
