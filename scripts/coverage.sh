#!/usr/bin/env bash
# scripts/coverage.sh — emit an lcov coverage report.
# Uses `cargo-llvm-cov` if available, otherwise emits a stub report.
# Informational only — does not fail CI.
set -euo pipefail

mkdir -p target/coverage
OUT=target/coverage/lcov.info

if command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "step: cargo-llvm-cov status: running"
    cargo llvm-cov --workspace --all-targets --lcov --output-path "$OUT"
    PCT=$(cargo llvm-cov --workspace --all-targets --summary-only 2>/dev/null \
        | awk '/^TOTAL/ { for (i=1; i<=NF; i++) if ($i ~ /%$/) { gsub("%","",$i); print $i; exit } }')
    echo "coverage: ${PCT:-unknown}%"
elif command -v cargo-tarpaulin >/dev/null 2>&1; then
    echo "step: cargo-tarpaulin status: running"
    cargo tarpaulin --workspace --out Lcov --output-dir target/coverage
    echo "coverage: unknown"
else
    echo "step: stub status: no coverage tool installed"
    cat > "$OUT" <<'EOF'
# No coverage tool available (install cargo-llvm-cov or cargo-tarpaulin).
# This stub exists so CI can upload an artifact without failing the build.
EOF
    echo "coverage: unknown"
fi