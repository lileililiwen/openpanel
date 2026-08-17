#!/usr/bin/env bash
# scripts/check-docs.sh — doc gate: broken intra-doc links fail the build.
#
# Bounded parallelism: `cargo doc` spawns one rustdoc per crate; on
# large workspaces that can exceed the memory budget of modest CI
# machines and the kernel SIGKILLs the build. Cap the job count so
# the gate is deterministic everywhere.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "doc" cargo doc --workspace --no-deps --jobs 2
