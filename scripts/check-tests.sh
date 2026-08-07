#!/usr/bin/env bash
# scripts/check-tests.sh — Run the full test suite and report results.
# Exit code 0 = all green, 1 = failures.
set -euo pipefail

echo "=== cargo check --workspace --all-targets ==="
cargo check --workspace --all-targets

echo ""
echo "=== cargo test --workspace --all-targets ==="
cargo test --workspace --all-targets -- --test-threads=1

echo ""
echo "=== All tests passed ==="
