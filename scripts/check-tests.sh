#!/usr/bin/env bash
# scripts/check-tests.sh — test gate: compile everything, then run the
# full test suite. Exits non-zero on any failure.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "test-check" cargo check --workspace --all-targets
step "test" cargo test --workspace --all-targets -- --test-threads=1
