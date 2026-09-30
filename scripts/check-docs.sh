#!/usr/bin/env bash
# scripts/check-docs.sh — documentation gate.
#
# Two legs:
#   1. `doc` — rustdoc intra-doc links (`cargo doc`). Bounded
#      parallelism: rustdoc spawns one process per crate; on large
#      workspaces that can exceed the memory budget of modest CI
#      machines and the kernel SIGKILLs the build. Cap the job count so
#      the gate is deterministic everywhere.
#   2. `docs-readiness` — the reader-facing surface promised by the
#      `readiness` capability:
#        * every in-repo relative link and image in README.md resolves;
#        * the generated capability catalog (`docs/CAPABILITIES.md`)
#          matches what `scripts/capabilities.sh` produces from the live
#          spec set, so it cannot drift silently.
#
# `--readiness-only` runs leg 2 alone. It exists for the governance
# self-test (`scripts/test-gates.sh`), which cannot invoke `cargo doc`
# inside a fixture tree. The gate itself (`make docs`, and the
# `driftwatchdog` `docs` check) invokes the script with no arguments, so
# both legs always run.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

README="${REPO_ROOT}/README.md"
CATALOG="docs/CAPABILITIES.md"
GENERATOR="scripts/capabilities.sh"

# Verify the reader-facing README assets and links, then the catalog.
docs_readiness() {
    local rc=0

    if [ ! -f "${README}" ]; then
        echo "docs-readiness: README not found: ${README}"
        return 1
    fi

    # Collect every markdown link/image target: `](target)`.
    local targets
    targets="$(grep -oE '\]\([^)]+\)' "${README}" | sed -E 's/^\]\(//; s/\)$//' || true)"

    local target path
    while IFS= read -r target; do
        [ -z "${target}" ] && continue
        case "${target}" in
            http://* | https://* | mailto:* | tel:* | '#'* | '//'*) continue ;;
        esac
        # Drop any fragment / query, and treat a root-absolute path as
        # repo-relative (README lives at the repo root).
        path="${target%%#*}"
        path="${path%%\?*}"
        path="${path#/}"
        [ -z "${path}" ] && continue
        if [ ! -e "${REPO_ROOT}/${path}" ]; then
            echo "docs-readiness: README references a missing path: ${target} (resolved: ${path})"
            rc=1
        fi
    done <<< "${targets}"

    # The capability catalog MUST be present and byte-identical to a
    # fresh regeneration from the live spec set.
    if [ ! -f "${CATALOG}" ]; then
        echo "docs-readiness: missing capability catalog: ${CATALOG}"
        rc=1
    fi

    local generated
    generated="$(mktemp)"
    if ! "${REPO_ROOT}/${GENERATOR}" > "${generated}" 2>/dev/null; then
        echo "docs-readiness: capability generator failed: ${GENERATOR}"
        rc=1
    elif [ -f "${CATALOG}" ] && ! diff -q "${generated}" "${CATALOG}" >/dev/null; then
        echo "docs-readiness: stale capability catalog: ${CATALOG} (regenerate: ${GENERATOR} > ${CATALOG})"
        rc=1
    fi
    rm -f "${generated}"

    return "${rc}"
}

if [ "${1:-}" != "--readiness-only" ]; then
    step "doc" cargo doc --workspace --no-deps --jobs 2
fi

step "docs-readiness" docs_readiness
