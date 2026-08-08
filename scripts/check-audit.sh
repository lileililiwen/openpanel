#!/usr/bin/env bash
# scripts/check-audit.sh — dependency audit gate.
# Optional: skipped with a clear status when cargo-audit is not installed.
#
# Uses `--no-fetch --stale` so the gate works in offline / restricted
# environments using the cached advisory-db. In CI with network access,
# run `cargo audit fetch` separately to refresh the cache first.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

if command -v cargo-audit >/dev/null 2>&1; then
    step "audit" cargo audit --no-fetch --stale
else
    echo ""
    echo "step: audit status: skipped (cargo-audit not installed)"
fi
