#!/usr/bin/env bash
# scripts/check-layering.sh — intra-crate DDD boundary gate.
#
# The four-layer DDD architecture is enforced STRUCTURALLY for the core
# crates (cargo refuses cross-layer deps), but new bounded contexts are
# FOLDERS inside openpanel-domain / openpanel-app, which cargo cannot
# isolate. This gate enforces the same direction rule at the folder
# level with a path-based check:
#   - no file under crates/openpanel-domain/src may `use openpanel_app`
#     or `use openpanel_api`
#   - no file under crates/openpanel-app/src may `use openpanel_api`
#
# A violation prints `step: layering status: failed` with the file and
# exits non-zero. Degrades to SKIPPED when `rg` is unavailable.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

# Overridable root for the workspace crates (used by the self-test).
CRATES_DIR="${OPENSPEC_CRATES_DIR:-crates}"

if ! command -v rg >/dev/null 2>&1; then
  echo ""
  echo "step: layering status: skipped (rg not installed)"
  exit 0
fi

violations=""

# Domain must not depend on app or api. rg recurses by default, so we
# point it at the domain src dir directly (no fragile ** glob).
while IFS= read -r f; do
  [ -z "${f}" ] && continue
  violations="${violations}  - ${f} (domain MUST NOT import app/api)\n"
done < <(rg --no-heading -N -g '*.rs' \
  -e 'use openpanel_app' -e 'use openpanel_api' \
  "${CRATES_DIR}/openpanel-domain/src" 2>/dev/null || true)

# App must not depend on api.
while IFS= read -r f; do
  [ -z "${f}" ] && continue
  violations="${violations}  - ${f} (app MUST NOT import api)\n"
done < <(rg --no-heading -N -g '*.rs' \
  -e 'use openpanel_api' "${CRATES_DIR}/openpanel-app/src" 2>/dev/null || true)

if [ -n "${violations}" ]; then
  echo ""
  echo "step: layering status: failed"
  echo "  DDD layering violation(s):"
  printf "%b" "${violations}"
  exit 1
fi

step "layering" true
