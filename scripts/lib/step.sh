#!/usr/bin/env bash
# scripts/lib/step.sh — shared helper sourced by every per-check script.
#
# Runs one command and prints a machine-parseable status line:
#   step: <name> status: ok | failed
#
# Exits non-zero on failure so the Makefile short-circuits the rest of
# the gate.
set -euo pipefail

step() {
    local name="$1"
    shift
    echo ""
    echo "step: ${name} status: running"
    if "$@"; then
        echo "step: ${name} status: ok"
    else
        echo "step: ${name} status: failed"
        return 1
    fi
}
