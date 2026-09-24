#!/usr/bin/env bash
# scripts/check-application-delivery.sh — application-delivery
# contract gate.
#
# Per the application-delivery spec, OpenPanel exposes a typed
# `Application` aggregate above the existing site Git,
# container, preview, and release services. The aggregate owns
# the immutable release, the production / preview environment
# split, the typed state machine, the promotion + rollback
# operators, and the secret-safe diagnostic contract.
#
# This gate is read-only and asserts the *wiring* of the
# contract:
#   - the live `application-delivery` spec is present,
#   - the domain module exposes the typed aggregate, the
#     environment kinds, the state machine, and the
#     redaction helper,
#   - the app service exists, exposes `register_application`,
#     `build_release`, `promote`, `rollback`, and routes audit
#     metadata through the canonical `redact_metadata` allowlist,
#   - the integration test file covers every scenario in the
#     spec,
#   - no `use openpanel_app` import appears in the domain
#     application-delivery module (layering invariant
#     re-asserted for the new bounded context).
#
# Failures are named (file:line); a clean run prints
# `step: application-delivery status: ok`. The gate degrades to
# `status: skipped` only when `rg` is unavailable; all other
# violations exit non-zero.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

# Overridable roots (used by the self-test). The env vars may be
# absolute or relative to the script's REPO_ROOT; treat absolute paths
# as final and prepend REPO_ROOT only to relative paths so the gate
# works both in the real repo and in a fixture tree.
CRATES_DIR_RAW="${OPENSPEC_CRATES_DIR:-crates}"
case "${CRATES_DIR_RAW}" in
  /*) CRATES_DIR="${CRATES_DIR_RAW}" ;;
  *)  CRATES_DIR="${REPO_ROOT}/${CRATES_DIR_RAW}" ;;
esac
SPECS_DIR_RAW="${OPENSPEC_SPECS_DIR:-openspec/specs}"
case "${SPECS_DIR_RAW}" in
  /*) SPECS_DIR="${SPECS_DIR_RAW}" ;;
  *)  SPECS_DIR="${REPO_ROOT}/${SPECS_DIR_RAW}" ;;
esac
TESTS_DIR_RAW="${OPENSPEC_TESTS_DIR:-tests/integration}"
case "${TESTS_DIR_RAW}" in
  /*) TESTS_DIR="${TESTS_DIR_RAW}" ;;
  *)  TESTS_DIR="${REPO_ROOT}/${TESTS_DIR_RAW}" ;;
esac

if ! command -v rg >/dev/null 2>&1; then
    echo ""
    echo "step: application-delivery status: skipped (rg not installed)"
    exit 0
fi

violations=""

record() { # record <message>
    violations="${violations}  - ${1}\n"
}

# --- 1. the live spec or the in-flight change delta is present ---------
# The `openspec archive` step merges the change folder's delta into the
# live spec at `openspec/specs/<cap>/spec.md`. The gate must run green
# both during implementation (when the delta is still in the change
# folder) and after archive (when the live spec exists). Accept either.
SPEC="${SPECS_DIR}/application-delivery/spec.md"
DELTA="${REPO_ROOT}/openspec/changes/add-git-application-delivery/specs/application-delivery/spec.md"
if [ ! -f "${SPEC}" ] && [ ! -f "${DELTA}" ]; then
    record "missing live spec (${SPEC}) and missing change delta (${DELTA})"
    # Without either spec source the scenario coverage check below
    # has nothing to assert against; short-circuit by reporting the
    # failure and letting the final report run.
    SPEC_FOR_SCENARIOS=""
elif [ -f "${SPEC}" ]; then
    SPEC_FOR_SCENARIOS="${SPEC}"
else
    SPEC_FOR_SCENARIOS="${DELTA}"
fi

# --- 2. the domain module exists and exposes the contract ---------------
# The application-delivery bounded context is split across
# `mod.rs` (module decl + re-exports), `types.rs` (the data
# shapes), and `logic.rs` (the lifecycle + adapters). The
# gate searches the whole directory so the split does not
# hide any of the required items.
DOMAIN_DIR="${CRATES_DIR}/openpanel-domain/src/application_delivery"
DOMAIN_MOD="${DOMAIN_DIR}/mod.rs"
if [ ! -f "${DOMAIN_MOD}" ]; then
    record "missing domain module: ${DOMAIN_MOD}"
else
    if ! rg -q 'pub struct Application\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub struct Application"
    fi
    if ! rg -q 'pub struct Environment\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub struct Environment"
    fi
    if ! rg -q 'pub struct Release\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub struct Release"
    fi
    if ! rg -q 'pub enum ReleaseState\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub enum ReleaseState"
    fi
    if ! rg -q 'pub enum EnvironmentKind\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub enum EnvironmentKind"
    fi
    if ! rg -q 'pub fn redact_diagnostic\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub fn redact_diagnostic"
    fi
    if ! rg -q 'pub fn is_mutable_digest\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub fn is_mutable_digest"
    fi
    if ! rg -q 'pub fn resolve_environment\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub fn resolve_environment"
    fi
    if ! rg -q 'pub fn can_promote\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub fn can_promote"
    fi
    if ! rg -q 'pub fn can_rollback\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub fn can_rollback"
    fi
    # Domain MUST NOT import app or api. The check recursively
    # walks the whole bounded-context directory.
    while IFS= read -r f; do
        [ -z "${f}" ] && continue
        record "${f} (domain MUST NOT import app/api)"
    done < <(rg --no-heading -N -g '*.rs' \
        -e 'use openpanel_app' -e 'use openpanel_api' \
        "${DOMAIN_DIR}" 2>/dev/null || true)
fi

# --- 3. the app service exists and exposes the lifecycle ---------------
APP_SERVICE="${CRATES_DIR}/openpanel-app/src/application_delivery/service.rs"
if [ ! -f "${APP_SERVICE}" ]; then
    record "missing app service: ${APP_SERVICE}"
else
    if ! rg -q 'pub struct ApplicationDeliveryService\b' "${APP_SERVICE}"; then
        record "app service does not define pub struct ApplicationDeliveryService"
    fi
    if ! rg -q 'pub fn register_application\b' "${APP_SERVICE}"; then
        record "app service does not expose pub fn register_application"
    fi
    if ! rg -q 'pub fn build_release\b' "${APP_SERVICE}"; then
        record "app service does not expose pub fn build_release"
    fi
    if ! rg -q 'pub fn promote\b' "${APP_SERVICE}"; then
        record "app service does not expose pub fn promote"
    fi
    if ! rg -q 'pub fn rollback\b' "${APP_SERVICE}"; then
        record "app service does not expose pub fn rollback"
    fi
    if ! rg -q 'fn require_operator\b' "${APP_SERVICE}"; then
        record "app service does not gate non-operator callers (missing require_operator)"
    fi
    if ! rg -q 'redact_metadata\b' "${APP_SERVICE}"; then
        record "app service does not route audit metadata through the canonical redact_metadata allowlist"
    fi
fi

# --- 4. the integration test file covers the scenarios -----------------
TEST_FILE="${TESTS_DIR}/application_delivery.rs"
if [ ! -f "${TEST_FILE}" ]; then
    record "missing integration test file: ${TEST_FILE}"
else
    # Every scenario in the live spec MUST be referenced by name in the
    # test file. We grep the spec for "#### Scenario:" lines, then
    # assert the test file mentions each scenario's leading text. This
    # keeps the coverage check live as the spec grows.
    while IFS= read -r scenario; do
        [ -z "${scenario}" ] && continue
        # scenario is a full "#### Scenario: <name>" line; reduce to a
        # fingerprint that the test file is expected to cite.
        fingerprint="${scenario#\#\#\#\# Scenario: }"
        if ! rg -qF "${fingerprint}" "${TEST_FILE}"; then
            record "test file does not reference scenario: ${scenario}"
        fi
    done < <(rg --no-heading -N '^#### Scenario: ' "${SPEC_FOR_SCENARIOS}" 2>/dev/null || true)
fi

# --- final report -------------------------------------------------------
if [ -n "${violations}" ]; then
    echo ""
    echo "step: application-delivery status: failed"
    echo "  application-delivery contract violation(s):"
    printf "%b" "${violations}"
    exit 1
fi

step "application-delivery" true
