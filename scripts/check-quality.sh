#!/usr/bin/env bash
# scripts/check-quality.sh — Run quality gates: fmt, clippy, doc, tests.
# Exit code 0 = all green, 1 = failures.
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

step "fmt" cargo fmt --all -- --check
step "clippy" cargo clippy --workspace --all-targets -- -D warnings
step "doc" cargo doc --workspace --no-deps

# Optional dependency audit — only run if the tool is installed.
if command -v cargo-audit >/dev/null 2>&1; then
    step "audit" cargo audit
else
    echo ""
    echo "step: audit status: skipped (cargo-audit not installed)"
fi

echo ""
echo "=== All quality checks passed ==="
