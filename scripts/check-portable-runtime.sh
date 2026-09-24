#!/usr/bin/env bash
# scripts/check-portable-runtime.sh — portable-runtime contract gate.
#
# Per the portable-runtime spec, the panel's runtime contract is the
# union of (1) a published target manifest, (2) a non-root, secret-
# free OCI image, (3) a native installer that preflights the manifest
# and refuses unsupported platforms, and (4) an upgrade path that
# verifies health after the swap and rolls back on failure.
#
# This gate checks the static artifacts: the Dockerfile MUST NOT
# contain credential literals, the target manifest MUST be present
# and parseable, the native installer MUST consult the manifest,
# and both adapters MUST agree on the persistent data directory.
# The runtime behavior (install/upgrade/rollback on a real host) is
# covered by packages/installer/install-tests.sh, which requires
# Docker and runs in a separate job.
#
# Environment:
#   OPENPANEL_PORTABLE_RUNTIME_REQUIRED=1
#       Fail closed when any contract violation is found. This is the
#       publication mode. The default (0) makes the gate advisory so
#       local development is not blocked.
#
# Exit codes:
#   0  — no violations found (or advisory mode)
#   1  — a contract violation was found in required mode
set -euo pipefail

# shellcheck source=lib/step.sh
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/step.sh
. "${SCRIPT_DIR}/lib/step.sh"

REPO_ROOT="${OPENPANEL_PORTABLE_RUNTIME_REPO_ROOT:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
REQUIRED="${OPENPANEL_PORTABLE_RUNTIME_REQUIRED:-0}"
DOCKERFILE="${REPO_ROOT}/Dockerfile"
INSTALL_SH="${REPO_ROOT}/packages/installer/install.sh"
ENTRYPOINT_SH="${REPO_ROOT}/packages/installer/entrypoint.sh"
TARGET_MANIFEST="${REPO_ROOT}/packages/installer/target-manifest.txt"

violations=0
record() { # record <label> <message>
    if [ "$1" = "fail" ]; then
        violations=$((violations + 1))
        printf '  portable-runtime: %s\n' "$2" >&2
    else
        printf '  portable-runtime: %s\n' "$2"
    fi
}

# --- 1. Dockerfile is free of credential literals ----------------------
# The portable-runtime spec (Persistent Data and Secret Boundary) says
# the image MUST NOT contain any password/api_key/token/secret literal,
# MUST NOT COPY a credentials file, and MUST NOT ARG-declare a secret.
scan_dockerfile() {
    if [ ! -f "${DOCKERFILE}" ]; then
        record fail "Dockerfile missing at ${DOCKERFILE}"
        return
    fi
    # A credential literal is one of:
    #   - password=...   (any value)
    #   - api_key=...    (any value)
    #   - token=...      (any value)
    #   - secret=...     (any value)
    #   - ARG <name>_PASSWORD / _TOKEN / _SECRET / _KEY
    #   - COPY ...credentials... or COPY ...secret...
    if grep -nE '(^|[[:space:]])(password|api_key|token|secret)[[:space:]]*=' \
            "${DOCKERFILE}"; then
        record fail "Dockerfile contains a credential literal (password=/api_key=/token=/secret=)"
    fi
    if grep -nE '^ARG[[:space:]]+[A-Z0-9_]*(PASSWORD|TOKEN|SECRET|API_KEY)' \
            "${DOCKERFILE}"; then
        record fail "Dockerfile declares a secret-shaped ARG"
    fi
    if grep -nE '^COPY[[:space:]].*(credentials|secret|password)' \
            "${DOCKERFILE}"; then
        record fail "Dockerfile COPYs a credentials-shaped path"
    fi
}

# --- 2. target-manifest.txt exists and is non-empty --------------------
check_target_manifest() {
    if [ ! -f "${TARGET_MANIFEST}" ]; then
        record fail "target manifest missing at ${TARGET_MANIFEST}"
        return
    fi
    local lines
    lines="$(grep -cE '^[a-z_]+[[:space:]]+(x86_64|aarch64)$' "${TARGET_MANIFEST}" || true)"
    if [ "${lines}" -lt 1 ]; then
        record fail "target manifest has no <id> <arch> entries"
    fi
}

# --- 3. install.sh consults the manifest -------------------------------
# A "consult" is any of: sourcing the manifest, grepping the manifest,
# reading the manifest via input redirection, or invoking a helper
# that does. The exact form is left to the implementation, but the
# manifest MUST be referenced; a hard-coded list is a spec violation
# per the "single source of truth" clause.
check_install_consults_manifest() {
    if [ ! -f "${INSTALL_SH}" ]; then
        record fail "install.sh missing at ${INSTALL_SH}"
        return
    fi
    if ! grep -qE 'target-manifest\.txt' "${INSTALL_SH}"; then
        record fail "install.sh does not reference target-manifest.txt"
    fi
    if ! grep -qE 'exit 78|EX_CONFIG' "${INSTALL_SH}"; then
        record fail "install.sh does not exit 78 (EX_CONFIG) on unsupported target"
    fi
}

# --- 4. both adapters agree on the persistent data dir ----------------
# The Dockerfile MUST declare the same default path the entrypoint
# uses when OPENPANEL_DATA_DIR is unset. A drift means the OCI image
# and the native installer do not satisfy the "equivalent adapters"
# scenario of the runtime contract.
check_data_dir_agreement() {
    if [ ! -f "${DOCKERFILE}" ] || [ ! -f "${ENTRYPOINT_SH}" ]; then
        return
    fi
    # Find the ENV line that sets OPENPANEL_DATA_DIR (or the ENV
    # OPENPANEL_DATA_DIR form). Use the first match.
    local docker_dir
    docker_dir="$(grep -E '^ENV[[:space:]]+OPENPANEL_DATA_DIR=' "${DOCKERFILE}" \
        | head -1 | sed -E 's/^ENV[[:space:]]+OPENPANEL_DATA_DIR=//')"
    local entrypoint_default
    entrypoint_default="$(grep -E 'OPENPANEL_DATA_DIR:-' "${ENTRYPOINT_SH}" \
        | head -1 | sed -E 's/.*OPENPANEL_DATA_DIR:-([^}"]+).*/\1/')"
    if [ -z "${docker_dir}" ] || [ -z "${entrypoint_default}" ]; then
        record fail "cannot determine default OPENPANEL_DATA_DIR in one of Dockerfile / entrypoint.sh"
        return
    fi
    if [ "${docker_dir}" != "${entrypoint_default}" ]; then
        record fail "OPENPANEL_DATA_DIR drift: Dockerfile=${docker_dir} entrypoint.sh=${entrypoint_default}"
    fi
}

# --- 5. upgrade path runs a post-upgrade smoke check ------------------
check_upgrade_smoke() {
    if [ ! -f "${INSTALL_SH}" ]; then
        return
    fi
    if ! grep -qE '(healthcheck|/health)' "${INSTALL_SH}"; then
        record fail "install.sh upgrade path does not run a post-upgrade health smoke check"
    fi
    if ! grep -qE 'restore|rollback|\.bak\.' "${INSTALL_SH}"; then
        record fail "install.sh upgrade path does not retain a recoverable previous binary"
    fi
}

scan_dockerfile
check_target_manifest
check_install_consults_manifest
check_data_dir_agreement
check_upgrade_smoke

if [ "${violations}" -gt 0 ]; then
    if [ "${REQUIRED}" = "1" ]; then
        step portable-runtime false
        exit 1
    fi
    # Advisory mode: print but do not fail.
    step portable-runtime true
    printf 'step: portable-runtime status: ok (advisory; %d violation(s) reported)\n' "${violations}"
    exit 0
fi

step portable-runtime true
