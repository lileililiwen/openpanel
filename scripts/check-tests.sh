#!/usr/bin/env bash
# scripts/check-tests.sh — test gate: compile everything, then run the
# full test suite. Exits non-zero on any failure.
#
# Tests are isolated by design (every test gets its own `TestDb` / temp
# dir and its own `TestServer` on a random port), so the suite is
# order-independent and safe to run with one worker per core. This is a
# stronger check of that contract than `--test-threads=1` and cuts the
# gate from ~30 minutes to a few minutes on a typical dev machine.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

step "test-check" cargo check --workspace --all-targets

# Cap the worker count: nproc on big CI boxes can be enormous, and each
# worker boots a full server + DB, so clamp to a sane ceiling.
THREADS="$(nproc 2>/dev/null || echo 2)"
if [ "${THREADS}" -gt 16 ]; then THREADS=16; fi

# `make check` already ran fmt/clippy/docs/audit in the parent before the
# test phase. Tell the quality canary not to re-run them inside the suite
# (a nested full compile would duplicate minutes of work and starve every
# other test of CPU under parallel execution). Standalone `cargo test`
# still runs the canary.
export OPENPANEL_TEST_SKIP_GATE_CANARY=1

step "test" cargo test --workspace --all-targets -- --test-threads="${THREADS}"
