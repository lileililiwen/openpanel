#!/usr/bin/env bash
# scripts/check-audit.sh — dependency audit gate (strict).
#
# Per the release-evidence spec (Audited Dependency Baseline), this
# gate MUST fail when the lockfile contains an actionable RustSec
# advisory and MUST NOT classify unmaintained / yanked / notice /
# unsound findings as vulnerabilities. Each actionable finding is
# reported as a single line so a maintainer can identify the crate,
# version, advisory, and remediation boundary without re-running
# `cargo audit` by hand.
#
# The gate parses the structured `cargo audit --json` output. When
# `OPENPANEL_AUDIT_BIN` is set, the script invokes that binary as
# the JSON source (used by the self-test fixtures). Otherwise the
# real `cargo audit --json --no-fetch --stale` is used so the gate
# works in offline / restricted environments with a cached advisory
# database.
#
# Local developer convenience: when `OPENPANEL_AUDIT_REQUIRED` is
# unset (or 0) and `cargo-audit` is not on PATH, the gate prints
# `step: audit status: skipped` and exits 0 so `make check` does
# not block unrelated work. When `OPENPANEL_AUDIT_REQUIRED=1` the
# gate fails closed if the tool is missing — this is the
# publication-job setting.
#
# The `ignore` list in `.cargo/audit.toml` is honoured: any advisory
# in that list is treated as a reviewed exception and is not counted
# as actionable. New ignores require a reviewed change.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

REQUIRED="${OPENPANEL_AUDIT_REQUIRED:-0}"
AUDIT_TOML="${REPO_ROOT}/.cargo/audit.toml"

# Build a Python source that loads a JSON object on stdin and emits
# a summary: "VULN\t<count>" on the first line and one
# "VULN\t<id>\t<crate>\t<version>\t<patched>" line per actionable
# advisory. Informational findings are emitted as "INFO\t<id>\t<kind>"
# so the script can re-classify them for the diagnostic block.
read_ignored() {
    if [ -f "${AUDIT_TOML}" ]; then
        awk -F'"' '/^[[:space:]]*ignore[[:space:]]*=/{for(i=2;i<=NF;i+=2)print $i}' \
            "${AUDIT_TOML}" | sort -u
    fi
}

run_audit_json() {
    if [ -n "${OPENPANEL_AUDIT_BIN:-}" ]; then
        # Fixture / override path: the binary is responsible for
        # emitting valid `cargo audit --json` on stdout.
        eval "${OPENPANEL_AUDIT_BIN}"
    elif command -v cargo-audit >/dev/null 2>&1; then
        cargo audit --json --no-fetch --stale
    else
        return 127
    fi
}

if ! json="$(run_audit_json 2>/dev/null)"; then
    rc=$?
    if [ "${REQUIRED}" = "1" ]; then
        echo ""
        echo "step: audit status: failed (cargo-audit unavailable and OPENPANEL_AUDIT_REQUIRED=1)"
        echo "  Install cargo-audit (cargo install --locked cargo-audit) so the gate can run."
        exit 1
    fi
    echo ""
    echo "step: audit status: skipped (cargo-audit not installed; set OPENPANEL_AUDIT_REQUIRED=1 to fail closed)"
    exit 0
fi

# Parse the JSON. python3 is the only JSON tool we assume (release.yml
# already requires it for SBOM generation). The classifier below keeps
# the same shape as the test-gates fixtures.
IGNORED="$(read_ignored 2>/dev/null || true)"
if [ -z "${IGNORED}" ]; then
    IGNORED_FILTER="__none__"
else
    IGNORED_FILTER="$(printf '%s\n' "${IGNORED}" | awk '{printf "%s|", $0}' | sed 's/|$//')"
fi

# Write the JSON to a temp file and the parser to a separate temp
# script, so stdin and argv are unambiguous. This keeps the gate
# robust against shell redirection quirks (heredoc + pipe / here-string
# ordering is not portable across bash versions).
json_tmp="$(mktemp)"
parser_tmp="$(mktemp)"
trap 'rm -f "${json_tmp}" "${parser_tmp}"' EXIT
printf '%s' "${json}" > "${json_tmp}"
cat > "${parser_tmp}" <<'PY'
import json, sys
ignored_filter = sys.argv[1]
ignored = set(filter(None, ignored_filter.split('|'))) if ignored_filter and ignored_filter != '__none__' else set()
with open(sys.argv[2], 'r', encoding='utf-8') as f:
    data = json.loads(f.read() or '{}')
vuln = (data.get('vulnerabilities') or {})
vuln_list = vuln.get('list') or []
info = data.get('warnings') or {}
informational = []
actionable = []
for v in vuln_list:
    a = v.get('advisory') or {}
    p = v.get('package') or {}
    versions = v.get('versions') or {}
    aid = a.get('id') or ''
    if aid in ignored:
        informational.append(('ignored', aid, p.get('name','?'), p.get('version','?'), ''))
        continue
    patched = versions.get('patched') or []
    actionable.append(('vuln', aid, p.get('name','?'), p.get('version','?'), ','.join(patched) or 'see-advisory'))
for kind, items in info.items():
    for it in items or []:
        a = it.get('advisory') or {}
        p = it.get('package') or {}
        aid = a.get('id') or ''
        if aid in ignored:
            continue
        informational.append((kind, aid, p.get('name','?'), p.get('version','?'), ''))
print(f"VULN\t{len(actionable)}")
for row in actionable:
    print('\t'.join(row))
print(f"INFO\t{len(informational)}")
for row in informational:
    print('\t'.join(row))
PY

summary="$(python3 "${parser_tmp}" "${IGNORED_FILTER}" "${json_tmp}" 2>/dev/null)" || summary=""

if [ -z "${summary}" ]; then
    echo ""
    echo "step: audit status: failed (cargo-audit output was not valid JSON)"
    exit 1
fi

vuln_count="$(printf '%s\n' "${summary}" | awk -F'\t' '$1=="VULN"{print $2; exit}')"
info_count="$(printf '%s\n' "${summary}" | awk -F'\t' '$1=="INFO"{print $2; exit}')"
vuln_count="${vuln_count:-0}"
info_count="${info_count:-0}"

if [ "${vuln_count}" -gt 0 ]; then
    echo ""
    echo "step: audit status: failed (${vuln_count} actionable advisory/ies)"
    printf '%s\n' "${summary}" \
        | awk -F'\t' '$1=="vuln"{
            printf "  - [FAIL] %s %s @ %s -> %s (advisory %s)\n", $3, $4, $2, $6, $2
        }'
    if [ "${info_count}" -gt 0 ]; then
        echo "  (informational: ${info_count} non-actionable warning(s) reported separately)"
        printf '%s\n' "${summary}" \
            | awk -F'\t' '$1=="unmaintained" || $1=="yanked" || $1=="notice" || $1=="unsound" || $1=="ignored"{
                printf "  - [info] %s %s (%s)\n", $3, $4, $2
            }'
    fi
    exit 1
fi

if [ "${info_count}" -gt 0 ]; then
    echo ""
    echo "step: audit status: ok (${info_count} informational warning(s) reported separately)"
    printf '%s\n' "${summary}" \
        | awk -F'\t' '
            $1=="unmaintained" || $1=="yanked" || $1=="notice" || $1=="unsound" || $1=="ignored" {
                printf "  - [info] %s %s (%s)\n", $3, $4, $2
            }'
    exit 0
fi

echo ""
echo "step: audit status: ok"
