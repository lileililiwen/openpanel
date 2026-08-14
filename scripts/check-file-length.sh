#!/usr/bin/env bash
# scripts/check-file-length.sh — per-file line-count lint.
#
# Runs `cargo lint-extra check` against the workspace. The thresholds
# are read from `lint-extra.toml` and the
# `OPENPANEL_FILE_LENGTH_SOFT_LIMIT` / `OPENPANEL_FILE_LENGTH_HARD_LIMIT`
# env vars via the `print-thresholds` binary (so the shell and the
# Rust app share one source of truth).
#
# Behaviour:
#  - If `cargo-lint-extra` is not on PATH, the step is SKIPPED
#    (mirrors the audit gate's graceful-skip behaviour).
#  - If `cargo-lint-extra` exits non-zero, the step FAILS and the
#    build short-circuits.
#  - If `make install-lint-tools` (run before this script) failed
#    to install the tool, the step is SKIPPED with a warning.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if ! command -v cargo-lint-extra >/dev/null 2>&1; then
    echo ""
    echo "step: file-length status: skipped (cargo-lint-extra not installed)"
    exit 0
fi

if [ -x "${REPO_ROOT}/target/debug/print_thresholds" ] || [ -x "${REPO_ROOT}/target/release/print_thresholds" ]; then
    bin="${REPO_ROOT}/target/debug/print_thresholds"
    [ -x "${REPO_ROOT}/target/release/print_thresholds" ] && bin="${REPO_ROOT}/target/release/print_thresholds"
    soft="$(cd "${REPO_ROOT}" && "${bin}" --soft)"
    hard="$(cd "${REPO_ROOT}" && "${bin}" --hard)"
    echo "thresholds: soft=${soft} hard=${hard}"
fi

# shellcheck disable=SC2317
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

# Run from the repo root so the .toml + .splitrs.toml config files
# are picked up. We disable every rule the spec does not cover so
# the file-length gate stays focused on file-length only.
cd "${REPO_ROOT}"
step file-length cargo lint-extra \
  --config ./cargo-lint-extra.toml \
  --disable line-length \
  --disable inline-comments \
  --disable todo-comments \
  --disable file-header \
  --disable allow-audit
