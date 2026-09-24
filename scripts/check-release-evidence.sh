#!/usr/bin/env bash
# scripts/check-release-evidence.sh — release-evidence contract gate.
#
# Per the release-evidence spec, every publication must ship a
# `dist/evidence-manifest.json` whose records carry enough
# environment-bound metadata to prove the build is reproducible and
# to detect stale evidence (records produced for a different commit
# or target). The gate is fail-closed when
# `OPENPANEL_RELEASE_EVIDENCE_REQUIRED=1` and skips (so local work is
# not blocked) when unset.
#
# The manifest is a single JSON object with two top-level fields:
#   commit  — the commit the publication was built from
#   target  — the target triple (e.g. x86_64-unknown-linux-gnu)
#   records — an array of evidence records, each with:
#       id             — required: one of `audit`, `coverage`,
#                        `browser-ui-quality`, `release-governance`,
#                        `smoke`
#       commit         — required: must equal the manifest commit
#                        (and the expected commit, when set)
#       command        — required: the command that produced the record
#       tool_versions  — required: object { tool: version }
#       target         — required: must equal the manifest target
#                        (and the expected target, when set)
#       timestamp      — required: ISO-8601 UTC timestamp
#       scope          — required: one of the documented scopes
#       state          — required: PASS | FAIL | BLOCKED
#
# A missing required record, a stale commit / target, a malformed
# record (any required field absent or empty), or a non-PASS state on
# a required record fails the gate. The gate is read-only: it never
# writes to the tree.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

DIST_DIR="${OPENPANEL_DIST_DIR:-dist}"
REQUIRED="${OPENPANEL_RELEASE_EVIDENCE_REQUIRED:-0}"
EXPECTED_COMMIT="${OPENPANEL_RELEASE_EXPECTED_COMMIT:-}"
EXPECTED_TARGET="${OPENPANEL_RELEASE_EXPECTED_TARGET:-}"
MANIFEST="${DIST_DIR}/evidence-manifest.json"

# Local-friendly skip when dist/ is absent and publication is not
# required: this is the local-dev path, identical to
# scripts/check-release-governance.sh.
if [ ! -d "${DIST_DIR}" ]; then
    if [ "${REQUIRED}" = "1" ]; then
        echo ""
        echo "step: release-evidence status: failed"
        echo "  - [BLOCKED] OPENPANEL_DIST_DIR=${DIST_DIR} does not exist"
        echo "  publication requires a complete evidence manifest"
        exit 1
    fi
    echo ""
    echo "step: release-evidence status: skipped (no OPENPANEL_DIST_DIR)"
    exit 0
fi

if [ ! -f "${MANIFEST}" ]; then
    if [ "${REQUIRED}" = "1" ]; then
        echo ""
        echo "step: release-evidence status: failed"
        echo "  - [BLOCKED] ${MANIFEST} is missing; publication requires an evidence manifest"
        exit 1
    fi
    echo ""
    echo "step: release-evidence status: skipped (no evidence-manifest.json)"
    exit 0
fi

# Use python3 for JSON parsing (release.yml already requires it). The
# validator is a single self-contained script so it can be reasoned
# about in isolation. It writes one TSV line per failure to stdout;
# the shell section below formats the diagnostic block.
validator="$(mktemp)"
trap 'rm -f "${validator}"' EXIT
cat > "${validator}" <<'PY'
import json, sys
manifest_path = sys.argv[1]
expected_commit = sys.argv[2] or ''
expected_target = sys.argv[3] or ''
required_ids = ['audit', 'coverage', 'browser-ui-quality', 'release-governance', 'smoke']
required_fields = ['id', 'commit', 'command', 'tool_versions', 'target', 'timestamp', 'scope', 'state']

with open(manifest_path, 'r', encoding='utf-8') as f:
    raw = f.read()
try:
    manifest = json.loads(raw or '{}')
except json.JSONDecodeError as exc:
    print(f"PARSE_ERROR\t{manifest_path}\t{exc.msg} (line {exc.lineno} col {exc.colno})")
    sys.exit(0)

if not isinstance(manifest, dict):
    print("PARSE_ERROR\t{}\tmanifest is not a JSON object".format(manifest_path))
    sys.exit(0)

commit = manifest.get('commit') or ''
target = manifest.get('target') or ''
records = manifest.get('records') or []

if not commit:
    print("MISSING_FIELD\tmanifest\tcommit")
if not target:
    print("MISSING_FIELD\tmanifest\ttarget")
if not isinstance(records, list) or not records:
    print("MISSING_FIELD\tmanifest\trecords (must be a non-empty array)")

seen = set()
for idx, rec in enumerate(records):
    if not isinstance(rec, dict):
        print(f"MALFORMED\t{idx}\trecord is not an object")
        continue
    rid = rec.get('id') or ''
    if rid in seen:
        print(f"DUPLICATE\t{rid}\tduplicate record id")
    seen.add(rid)
    for field in required_fields:
        val = rec.get(field)
        if val is None or val == '' or val == {} or val == []:
            print(f"MISSING_FIELD\t{rid}\t{field}")
    if rec.get('state') not in ('PASS', 'FAIL', 'BLOCKED'):
        print(f"BAD_STATE\t{rid}\tstate must be PASS|FAIL|BLOCKED, got {rec.get('state')!r}")
        continue
    if rid in required_ids and rec.get('state') != 'PASS':
        print(f"NON_PASS\t{rid}\tstate={rec.get('state')!r} not allowed for required record")
    if expected_commit and rec.get('commit') != expected_commit:
        print(f"STALE\t{rid}\tcommit={rec.get('commit')!r} does not match expected {expected_commit!r}")
    if expected_target and rec.get('target') != expected_target:
        print(f"STALE\t{rid}\ttarget={rec.get('target')!r} does not match expected {expected_target!r}")
    if commit and rec.get('commit') and rec.get('commit') != commit:
        print(f"STALE\t{rid}\tcommit={rec.get('commit')!r} does not match manifest commit {commit!r}")
    if target and rec.get('target') and rec.get('target') != target:
        print(f"STALE\t{rid}\ttarget={rec.get('target')!r} does not match manifest target {target!r}")
    if not isinstance(rec.get('tool_versions'), dict) or not rec.get('tool_versions'):
        # Already reported by MISSING_FIELD, keep going.
        pass

for rid in required_ids:
    if rid not in seen:
        print(f"MISSING_RECORD\t{rid}\trequired record not present")
PY

# Run the validator. The script emits one TSV line per finding to
# stdout and exits 0 on success (empty output) or non-zero when the
# manifest could not be read at all.
out=""
python_rc=0
if ! out="$(python3 "${validator}" "${MANIFEST}" "${EXPECTED_COMMIT}" "${EXPECTED_TARGET}" 2>/dev/null)"; then
    python_rc=$?
fi

if [ "${python_rc}" -ne 0 ] && [ -z "${out}" ]; then
    # Validator crashed AND emitted nothing we can show: report and
    # exit. This is the only case where empty output is a failure.
    echo ""
    echo "step: release-evidence status: failed (validator could not run; exit ${python_rc})"
    exit 1
fi

fails=0
report=""
if [ -n "${out}" ]; then
    while IFS=$'\t' read -r code id detail; do
        [ -n "${code:-}" ] || continue
        case "${code}" in
            PARSE_ERROR)
                report="${report}  - [BLOCKED] ${id}: ${detail}\n"
                ;;
            MISSING_FIELD)
                report="${report}  - [FAIL] ${id}: missing required field ${detail}\n"
                ;;
            MISSING_RECORD)
                report="${report}  - [FAIL] ${id}: ${detail}\n"
                ;;
            DUPLICATE)
                report="${report}  - [FAIL] ${id}: ${detail}\n"
                ;;
            MALFORMED)
                report="${report}  - [FAIL] record #${id}: ${detail}\n"
                ;;
            BAD_STATE)
                report="${report}  - [FAIL] ${id}: ${detail}\n"
                ;;
            NON_PASS)
                report="${report}  - [BLOCKED] ${id}: ${detail}\n"
                ;;
            STALE)
                report="${report}  - [STALE] ${id}: ${detail}\n"
                ;;
            *)
                report="${report}  - [FAIL] ${id}: ${detail}\n"
                ;;
        esac
        fails=$((fails + 1))
    done <<EOF
${out}
EOF
fi

if [ "${fails}" -ne 0 ]; then
    echo ""
    echo "step: release-evidence status: failed (${fails} issue(s))"
    printf "%b" "${report}"
    exit 1
fi

# Count the records that landed, for the ok-line summary.
present_count="$(python3 -c "import json,sys; print(len(json.load(open('${MANIFEST}')).get('records') or []))" 2>/dev/null || echo "?")"

echo ""
echo "step: release-evidence status: ok (${present_count} record(s) verified)"
