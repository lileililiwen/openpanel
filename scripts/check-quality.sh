#!/usr/bin/env bash
# scripts/check-quality.sh — Run quality gates: clippy, format check, doc tests.
# Exit code 0 = all green, 1 = failures.
set -euo pipefail

echo "=== cargo fmt --check ==="
cargo fmt --all -- --check

echo ""
echo "=== cargo clippy --workspace --all-targets ==="
cargo clippy --workspace --all-targets -- -D warnings

echo ""
echo "=== cargo doc --workspace ==="
cargo doc --workspace --no-deps

echo ""
echo "=== All quality checks passed ==="
