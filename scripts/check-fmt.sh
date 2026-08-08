#!/usr/bin/env bash
# scripts/check-fmt.sh — format gate: `cargo fmt --all -- --check`.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "fmt" cargo fmt --all -- --check
