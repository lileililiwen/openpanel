#!/usr/bin/env bash
# scripts/check-deployment-adapters.sh — deployment-adapter contract gate.
#
# Per the deployment-adapters spec, OpenPanel exposes a typed
# `DeploymentAdapter` trait and a `DeploymentAdapterService` that the
# API, the CLI, and the integration tests share. Adapters declare a
# manifest, idempotency is enforced through an `OperationKey`, and
# evidence is provider-neutral (no host path, no transport command,
# no secret value).
#
# This gate is read-only and asserts the *wiring* of the contract:
#   - the live `deployment-adapters` spec is present,
#   - the domain module exposes the trait and the typed data
#     (AdapterManifest, DeploymentPlan, DeploymentEvidence),
#   - the app service exists, exposes `run`, validates, and rejects
#     non-operator callers,
#   - the integration test file covers every scenario in the spec,
#   - no `use openpanel_app` import appears in the domain
#     deployment-adapters module (layering invariant re-asserted for
#     the new bounded context).
#
# Failures are named (file:line); a clean run prints
# `step: deployment-adapters status: ok`. The gate degrades to
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
  echo "step: deployment-adapters status: skipped (rg not installed)"
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
SPEC="${SPECS_DIR}/deployment-adapters/spec.md"
DELTA="${REPO_ROOT}/openspec/changes/add-portable-deployment-adapters/specs/deployment-adapters/spec.md"
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
# The deployment-adapters bounded context is split across
# `mod.rs` (module decl + re-exports), `types.rs` (the data
# shapes), and `logic.rs` (the trait + pure helpers). The
# gate searches the whole directory so the split does not
# hide any of the four required items.
DOMAIN_DIR="${CRATES_DIR}/openpanel-domain/src/deployment_adapters"
DOMAIN_MOD="${DOMAIN_DIR}/mod.rs"
if [ ! -f "${DOMAIN_MOD}" ]; then
    record "missing domain module: ${DOMAIN_MOD}"
else
    if ! rg -q 'pub trait DeploymentAdapter\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub trait DeploymentAdapter"
    fi
    if ! rg -q 'pub struct AdapterManifest\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub struct AdapterManifest"
    fi
    if ! rg -q 'pub struct DeploymentPlan\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub struct DeploymentPlan"
    fi
    if ! rg -q 'pub struct DeploymentEvidence\b' "${DOMAIN_DIR}"; then
        record "domain module does not define pub struct DeploymentEvidence"
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
APP_SERVICE="${CRATES_DIR}/openpanel-app/src/deployment_adapters/service.rs"
if [ ! -f "${APP_SERVICE}" ]; then
    record "missing app service: ${APP_SERVICE}"
else
    if ! rg -q 'pub struct DeploymentAdapterService\b' "${APP_SERVICE}"; then
        record "app service does not define pub struct DeploymentAdapterService"
    fi
    if ! rg -q 'pub async fn run\b' "${APP_SERVICE}"; then
        record "app service does not expose pub async fn run"
    fi
    if ! rg -q 'fn require_operator\b' "${APP_SERVICE}"; then
        record "app service does not gate non-operator callers (missing require_operator)"
    fi
    if ! rg -q 'redact_metadata\b' "${APP_SERVICE}"; then
        record "app service does not route audit metadata through the canonical redact_metadata allowlist"
    fi
fi

# --- 4. the integration test file covers the scenarios -----------------
TEST_FILE="${TESTS_DIR}/deployment_adapters.rs"
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
    echo "step: deployment-adapters status: failed"
    echo "  deployment-adapter contract violation(s):"
    printf "%b" "${violations}"
    exit 1
fi

step "deployment-adapters" true
