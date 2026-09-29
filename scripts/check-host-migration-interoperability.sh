#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"
CRATES_DIR_RAW="${OPENSPEC_CRATES_DIR:-crates}"
case "${CRATES_DIR_RAW}" in /*) CRATES_DIR="${CRATES_DIR_RAW}" ;; *) CRATES_DIR="${REPO_ROOT}/${CRATES_DIR_RAW}" ;; esac
SPECS_DIR_RAW="${OPENSPEC_SPECS_DIR:-openspec/specs}"
case "${SPECS_DIR_RAW}" in /*) SPECS_DIR="${SPECS_DIR_RAW}" ;; *) SPECS_DIR="${REPO_ROOT}/${SPECS_DIR_RAW}" ;; esac
TESTS_DIR_RAW="${OPENSPEC_TESTS_DIR:-tests/integration}"
case "${TESTS_DIR_RAW}" in /*) TESTS_DIR="${TESTS_DIR_RAW}" ;; *) TESTS_DIR="${REPO_ROOT}/${TESTS_DIR_RAW}" ;; esac
if ! command -v rg >/dev/null 2>&1; then echo ""; echo "step: host-migration-interoperability status: skipped (rg not installed)"; exit 0; fi
violations=""
record() { violations="${violations}  - ${1}\n"; }
SPEC="${SPECS_DIR}/host-migration-interoperability/spec.md"
DELTA="${REPO_ROOT}/openspec/changes/add-portable-host-operations-and-migration/specs/host-migration-interoperability/spec.md"
if [ ! -f "${SPEC}" ] && [ ! -f "${DELTA}" ]; then record "missing live spec (${SPEC}) and missing change delta (${DELTA})"; SPEC_FOR_SCENARIOS=""; elif [ -f "${SPEC}" ]; then SPEC_FOR_SCENARIOS="${SPEC}"; else SPEC_FOR_SCENARIOS="${DELTA}"; fi
DOMAIN_AGENT="${CRATES_DIR}/openpanel-domain/src/agent/mod.rs"
if [ ! -f "${DOMAIN_AGENT}" ]; then record "missing domain agent module: ${DOMAIN_AGENT}"; else
  if ! rg -q 'pub struct HostCapability\b' "${DOMAIN_AGENT}"; then record "domain agent module does not define pub struct HostCapability"; fi
  if ! rg -q 'pub enum HostHealth\b' "${DOMAIN_AGENT}"; then record "domain agent module does not define pub enum HostHealth"; fi
  if ! rg -q 'CapabilityMismatch' "${DOMAIN_AGENT}"; then record "domain agent module does not define CapabilityMismatch"; fi
fi
DOMAIN_MIG="${CRATES_DIR}/openpanel-domain/src/migration_importers/mod.rs"
if [ ! -f "${DOMAIN_MIG}" ]; then record "missing domain migration_importers module: ${DOMAIN_MIG}"; else
  if ! rg -q 'pub enum ConflictPolicy\b' "${DOMAIN_MIG}"; then record "domain migration_importers does not define pub enum ConflictPolicy"; fi
  if ! rg -q 'PartiallyCompleted' "${DOMAIN_MIG}"; then record "domain migration_importers does not define PartiallyCompleted status"; fi
fi
APP_HOST_ADAPTER="${CRATES_DIR}/openpanel-app/src/agent/host_capability.rs"
if [ ! -f "${APP_HOST_ADAPTER}" ]; then record "missing app host adapter: ${APP_HOST_ADAPTER}"; else
  if ! rg -q 'pub trait HostAdapter\b' "${APP_HOST_ADAPTER}"; then record "app host_ops does not define pub trait HostAdapter"; fi
fi
APP_HOST="${CRATES_DIR}/openpanel-app/src/host_ops"
if [ ! -d "${APP_HOST}" ]; then record "missing app host_ops module: ${APP_HOST}"; else
  if ! rg -q 'pub struct CutoverService\b' "${APP_HOST}"; then record "app host_ops does not define pub struct CutoverService"; fi
fi
APP_VERIFIERS="${CRATES_DIR}/openpanel-app/src/migration_importers/verifiers/mod.rs"
if [ ! -f "${APP_VERIFIERS}" ]; then record "missing app verifiers module: ${APP_VERIFIERS}"; else
  if ! rg -q 'pub trait PostImportVerifier\b' "${APP_VERIFIERS}"; then record "app verifiers does not define pub trait PostImportVerifier"; fi
fi
APP_CPANEL="${CRATES_DIR}/openpanel-app/src/migration_importers/drivers/cpanel/mod.rs"
if [ ! -f "${APP_CPANEL}" ]; then record "missing cpanel driver: ${APP_CPANEL}"; else
  if ! rg -q 'CpanelPkgacctDriver' "${APP_CPANEL}"; then record "cpanel driver does not define CpanelPkgacctDriver"; fi
  if ! rg -q 'CpanelLegacyBackupDriver' "${APP_CPANEL}"; then record "cpanel driver does not define CpanelLegacyBackupDriver"; fi
fi
APP_BAOTA="${CRATES_DIR}/openpanel-app/src/migration_importers/drivers/baota/mod.rs"
if [ ! -f "${APP_BAOTA}" ]; then record "missing baota driver: ${APP_BAOTA}"; else
  if ! rg -q 'BaotaBackupDriver' "${APP_BAOTA}"; then record "baota driver does not define BaotaBackupDriver"; fi
fi
if [ -n "${SPEC_FOR_SCENARIOS}" ] && [ -f "${SPEC_FOR_SCENARIOS}" ]; then
  TEST_FILE_HOST="${TESTS_DIR}/host_ops.rs"
  TEST_FILE_MIG="${TESTS_DIR}/migration_importers.rs"
  while IFS= read -r scenario; do [ -z "${scenario}" ] && continue; fingerprint="${scenario#\#\#\#\# Scenario: }"; found=false
    if [ -f "${TEST_FILE_HOST}" ] && rg -qF "${fingerprint}" "${TEST_FILE_HOST}"; then found=true; fi
    if [ -f "${TEST_FILE_MIG}" ] && rg -qF "${fingerprint}" "${TEST_FILE_MIG}"; then found=true; fi
    if [ "${found}" = false ]; then record "no test file references scenario: ${scenario}"; fi
  done < <(rg --no-heading -N '^#### Scenario: ' "${SPEC_FOR_SCENARIOS}" 2>/dev/null || true)
fi
if [ -n "${violations}" ]; then echo ""; echo "step: host-migration-interoperability status: failed"; echo "  host-migration-interoperability violation(s):"; printf "%b" "${violations}"; exit 1; fi
step "host-migration-interoperability" true
