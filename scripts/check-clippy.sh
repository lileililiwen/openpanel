#!/usr/bin/env bash
# scripts/check-clippy.sh — lint gate: clippy denies all warnings.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "clippy" cargo clippy --workspace --all-targets -- -D warnings
