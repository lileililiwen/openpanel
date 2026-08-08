#!/usr/bin/env bash
# scripts/check-docs.sh — doc gate: broken intra-doc links fail the build.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "doc" cargo doc --workspace --no-deps
