#!/usr/bin/env bash
# scripts/check-declared-verification.sh — declared-verifier wiring gate.
#
# `.project.json verification.command` is the project's DECLARED
# verifier (`cargo test --workspace`); driftwatchdog runs it as the
# `declared-verification` command. This `make check` step does NOT
# re-run the suite (that would duplicate `make test`, whose
# `cargo test --workspace --all-targets` is already a superset).
# Instead it machine-verifies the coverage claim in both directions:
#   1. `.project.json` declares exactly `cargo test --workspace`.
#   2. `scripts/check-tests.sh` (the `make test` binding) runs
#      `cargo test --workspace` with at least `--all-targets`.
# Either side drifting without the other fails loudly here instead
# of silently uncovering the declared verification.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

declared_ok() {
    python3 - "${REPO_ROOT}/.project.json" "${REPO_ROOT}/scripts/check-tests.sh" <<'PY'
import sys

project_path, tests_path = sys.argv[1], sys.argv[2]

import json

with open(project_path) as f:
    project = json.load(f)
declared = ((project.get("verification") or {}).get("command") or "").strip()
if declared != "cargo test --workspace":
    print(f"declared verifier is {declared!r}, expected 'cargo test --workspace'")
    sys.exit(1)

with open(tests_path) as f:
    tests_script = f.read()
if "cargo test --workspace" not in tests_script:
    print("scripts/check-tests.sh no longer runs `cargo test --workspace`")
    sys.exit(1)
if "--all-targets" not in tests_script:
    print("scripts/check-tests.sh no longer runs the --all-targets superset")
    sys.exit(1)
PY
}

step "declared-verification" declared_ok
