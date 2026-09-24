#!/usr/bin/env bash
# scripts/check-tests-gate.sh — the driftwatchdog `tests` binding.
#
# driftwatchdog 0.1.0 enforces a hard 60 s per-command budget
# (CHECK_TIMEOUT_MS in src/commands/gate.rs, no per-command
# override), and the full `scripts/check-tests.sh` suite (unit +
# integration + doc tests) cannot fit it. This wrapper runs only the
# workspace UNIT suites (`--lib`), which machine-decides in ~35-42 s
# on a warm cache (~3400 tests across all crates).
#
# The heavy integration + doc suite stays in `make test`
# (`scripts/check-tests.sh`), i.e. the full developer / CI chain. A
# cold-cache run of this wrapper is killed by the budget and surfaces
# as a gate "pending review"; the operator resolves it by running
# `make test` to completion (which also warms the cache) before
# archive. See openspec/changes/add-driftwatchdog-gate-migration/design.md
# §"Verified driftwatchdog 0.1.0 runtime behavior".
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "test-gate" cargo test --workspace --lib --no-fail-fast
