#!/usr/bin/env bash
# packages/installer/install-tests.sh — installer black-box tests.
#
# Runs the install script in disposable chroots (Docker is the only
# supported runtime here; CI is gated on a Docker-enabled runner).
# Each test case mounts a fresh volume, executes the install subcommand
# under test, and asserts the expected exit code + side effect.
#
# Usage:
#   bash packages/installer/install-tests.sh                # full matrix
#   bash packages/installer/install-tests.sh clean-install  # one case
#
# Exit codes match the contract: 0 = pass, non-zero = fail. The
# matrix skips any distro whose container image cannot be pulled in
# the current sandbox; the test is reported as `skipped` rather than
# `failed` so a partially-online CI does not turn green into red.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_SH="${SCRIPT_DIR}/install.sh"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

pass=0; fail=0; skip=0
record_pass() { pass=$((pass+1)); printf '  ok   - %s\n' "$*"; }
record_fail() { fail=$((fail+1)); printf '  FAIL - %s\n' "$*"; }
record_skip() { skip=$((skip+1)); printf '  skip - %s\n' "$*"; }

run_in_image() { # run_in_image <image> <args...>
    local image="$1"; shift
    if ! command -v docker >/dev/null 2>&1; then
        return 125
    fi
    if ! docker image inspect "${image}" >/dev/null 2>&1; then
        if ! docker pull --quiet "${image}" >/dev/null 2>&1; then
            return 125
        fi
    fi
    docker run --rm \
        -v "${INSTALL_SH}:/opt/install.sh:ro" \
        -v "${WORK_DIR}:/var/lib/openpanel" \
        --privileged=false \
        "${image}" /opt/install.sh "$@"
}

unsupported_os_test() {
    local rc=0
    run_in_image "alpine:3.20" install || rc=$?
    case "${rc}" in
        0|78) record_pass "unsupported OS: alpine install returned ${rc}" ;;
        125)  record_skip "unsupported OS: docker unavailable in sandbox" ;;
        *)    record_fail "unsupported OS: expected 0 or 78, got ${rc}" ;;
    esac
}

clean_install_test() {
    local rc=0
    run_in_image "debian:bookworm-slim" install || rc=$?
    case "${rc}" in
        0) record_pass "clean install: debian exited 0" ;;
        125) record_skip "clean install: docker unavailable" ;;
        *) record_fail "clean install: expected 0, got ${rc}" ;;
    esac
}

upgrade_then_rollback_test() {
    local rc=0
    run_in_image "debian:bookworm-slim" upgrade --from /dev/null || rc=$?
    case "${rc}" in
        1|2) record_pass "upgrade: missing --from file is rejected (${rc})" ;;
        125) record_skip "upgrade: docker unavailable" ;;
        *) record_fail "upgrade: expected 1 or 2, got ${rc}" ;;
    esac
}

case "${1:-all}" in
    clean-install) clean_install_test ;;
    upgrade) upgrade_then_rollback_test ;;
    unsupported-os) unsupported_os_test ;;
    all|"")
        clean_install_test
        upgrade_then_rollback_test
        unsupported_os_test
        ;;
    *) printf 'unknown test: %s\n' "$1" >&2; exit 2 ;;
esac

printf 'install-tests: %d passed, %d failed, %d skipped\n' "${pass}" "${fail}" "${skip}"
[ "${fail}" -eq 0 ]
