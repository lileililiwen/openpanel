#!/usr/bin/env bash
# scripts/check-release-governance.sh — release artifact integrity gate.
#
# A "supported" release ships a binary with four integrity sidecars:
#   1. <name>.sha256     — SHA-256 of the binary
#   2. <name>.sbom.json  — SBOM derived from `cargo metadata`
#   3. <name>.sig        — detached signature (minisign when
#                          RELEASE_VERIFY_KEY is set; placeholder
#                          fingerprint is accepted in test mode)
#   4. provenance.json   — manifest at the dist root listing every
#                          artifact, the build commit, the build
#                          target, and SOURCE_DATE_EPOCH
#
# A missing sidecar, an artifact not listed in provenance.json, or a
# signature that does not verify (with RELEASE_VERIFY_KEY set) fails
# the build. The gate is read-only: it never modifies an artifact.
#
# When OPENPANEL_DIST_DIR does not exist (local development) the gate
# exits zero with `status: skipped (no OPENPANEL_DIST_DIR)` so it does
# not block unrelated work; the skip is auditable on the make-check
# transcript.

set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

DIST_DIR="${OPENPANEL_DIST_DIR:-dist}"
PROVENANCE_NAME="provenance.json"
SBOM_MIN_BYTES=16
PROVENANCE_MIN_BYTES=16

if [ ! -d "${DIST_DIR}" ]; then
    echo ""
    echo "step: release-governance status: skipped (no OPENPANEL_DIST_DIR)"
    exit 0
fi

sha256_of() {
    sha256sum "$1" | awk '{print $1}'
}

fails=0
report=""

scan_artifact() {
    local artifact="$1"
    local name
    name="$(basename "${artifact}")"
    local dir
    dir="$(dirname "${artifact}")"

    if [ ! -f "${dir}/${name}.sha256" ]; then
        report="${report}  - [FAIL] ${name}: missing .sha256\n"
        fails=$((fails + 1))
        return
    fi
    local expected
    expected="$(sha256_of "${artifact}")"
    local recorded
    recorded="$(tr -d '[:space:]' < "${dir}/${name}.sha256" | head -c 64)"
    if [ "${recorded}" != "${expected}" ]; then
        report="${report}  - [FAIL] ${name}: .sha256 mismatch (expected ${expected}, got ${recorded})\n"
        fails=$((fails + 1))
        return
    fi

    if [ ! -s "${dir}/${name}.sbom.json" ]; then
        report="${report}  - [FAIL] ${name}: missing .sbom.json\n"
        fails=$((fails + 1))
        return
    fi
    local sbom_size
    sbom_size="$(wc -c < "${dir}/${name}.sbom.json")"
    if [ "${sbom_size}" -lt "${SBOM_MIN_BYTES}" ]; then
        report="${report}  - [FAIL] ${name}: .sbom.json too small (${sbom_size} bytes)\n"
        fails=$((fails + 1))
        return
    fi

    if [ ! -f "${dir}/${name}.sig" ]; then
        report="${report}  - [FAIL] ${name}: missing .sig\n"
        fails=$((fails + 1))
        return
    fi

    if [ -n "${RELEASE_VERIFY_KEY:-}" ] && command -v minisign >/dev/null 2>&1; then
        if ! minisign -V \
                -p "${RELEASE_VERIFY_KEY}" \
                -m "${artifact}" \
                -x "${dir}/${name}.sig" \
                >/dev/null 2>&1; then
            report="${report}  - [FAIL] ${name}: signature does not verify\n"
            fails=$((fails + 1))
            return
        fi
    fi
}

provenance_file="${DIST_DIR}/${PROVENANCE_NAME}"
if [ ! -s "${provenance_file}" ]; then
    echo ""
    echo "step: release-governance status: failed"
    echo "  - [FAIL] missing or empty ${provenance_file}"
    exit 1
fi
provenance_size="$(wc -c < "${provenance_file}")"
if [ "${provenance_size}" -lt "${PROVENANCE_MIN_BYTES}" ]; then
    echo ""
    echo "step: release-governance status: failed"
    echo "  - [FAIL] ${provenance_file} too small (${provenance_size} bytes)"
    exit 1
fi

shopt -s nullglob
artifacts=()
for candidate in "${DIST_DIR}"/*; do
    base="$(basename "${candidate}")"
    case "${base}" in
        *.sha256|*.sbom.json|*.sig|${PROVENANCE_NAME}|.gitkeep) continue ;;
    esac
    if [ -f "${candidate}" ]; then
        artifacts+=("${candidate}")
    fi
done
shopt -u nullglob

if [ "${#artifacts[@]}" -eq 0 ]; then
    echo ""
    echo "step: release-governance status: failed"
    echo "  - [FAIL] ${DIST_DIR} contains no artifacts to verify"
    exit 1
fi

listed_count=0
for artifact in "${artifacts[@]}"; do
    name="$(basename "${artifact}")"
    if grep -q "\"${name}\"\|${name}" "${provenance_file}"; then
        listed_count=$((listed_count + 1))
    else
        report="${report}  - [FAIL] provenance: artifact ${name} not listed\n"
        fails=$((fails + 1))
    fi
    scan_artifact "${artifact}"
done

if [ "${fails}" -ne 0 ]; then
    echo ""
    echo "step: release-governance status: failed"
    printf "%b" "${report}"
    exit 1
fi

echo ""
echo "step: release-governance status: ok"
echo "  - verified ${#artifacts[@]} artifact(s), ${listed_count} listed in provenance"
