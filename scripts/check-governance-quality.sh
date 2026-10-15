#!/usr/bin/env bash
# scripts/check-governance-quality.sh — workspace governance-quality gate.
#
# Runs the workspace-governance `workspace_check.py` project audit for
# this repository (the `governance-quality` driftwatchdog binding).
# The sibling `workspace-governance` checkout is a local portfolio
# tool, not a repo dependency: when it is absent (fresh clone, CI
# without the portfolio layout) the gate skips audibly with exit 0,
# following the `release-governance` / `coverage-floor` skip idiom,
# instead of blocking unrelated work.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

GOV_SCRIPT="${REPO_ROOT}/../workspace-governance/scripts/workspace_check.py"

if [ ! -f "${GOV_SCRIPT}" ]; then
    echo "step: governance-quality status: skipped (no ../workspace-governance checkout)"
    exit 0
fi

# driftwatchdog's governance concern parses this script's stdout as one
# JSON report, so the step markers go to stderr and only the report
# reaches stdout.
echo "step: governance-quality status: running" >&2
if python3 "${GOV_SCRIPT}" --root "${REPO_ROOT}/.." --project openpanel --json; then
    echo "step: governance-quality status: ok" >&2
else
    echo "step: governance-quality status: failed" >&2
    exit 1
fi
