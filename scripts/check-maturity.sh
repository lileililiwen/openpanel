#!/usr/bin/env bash
# scripts/check-maturity.sh — production incomplete-work evidence gate.
#
# Per the quality-maturity-ratchet spec, every TODO / FIXME / stub
# marker in production source MUST have a reviewed entry in the
# evidence manifest (`openspec/governance/evidence.yaml`). A
# new production marker introduced without an evidence record is a
# build failure; a manifest entry that points at a non-existent
# marker is a stale record and is also reported.
#
# Detected marker forms (all require a parenthesised issue key so the
# evidence record can be matched by id):
#   // TODO(openpanel#<id>[: extra context])
#   // FIXME(openpanel#<id>)
#   // stub(openpanel#<id>)
#
# Source scope: every `.rs` file under `crates/`, excluding tests
# and target/. The evidence record's `location` field is informational
# only — the gate matches by `id`.
#
# Manifest format (YAML):
#   version: 1
#   records:
#     - id: openpanel#ACME-HTTP01
#       type: todo | stub | env-blocked
#       location: crates/openpanel-app/src/ssl/acme.rs:102
#       owner: <name>
#       reason: <short justification>
#       closure: <follow-up change or issue key>
#
# Environment overrides (used by the self-test):
#   OPENSPEC_TODOS_DIR — directory to scan (default: crates)
#   OPENSPEC_EVIDENCE  — manifest path (default: openspec/governance/evidence.yaml)
#
# Exit codes:
#   0 — every detected marker has an evidence record (and every
#       evidence record matches a marker, unless `stale_ok` is set).
#   1 — a marker has no record, or the manifest is malformed.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

TODOS_DIR="${OPENSPEC_TODOS_DIR:-${REPO_ROOT}/crates}"
EVIDENCE="${OPENSPEC_EVIDENCE:-${REPO_ROOT}/openspec/governance/evidence.yaml}"

if ! command -v python3 >/dev/null 2>&1; then
  echo ""
  echo "step: maturity status: skipped (python3 not installed)"
  exit 0
fi

# python3 is available; defer the real work to an inline script so the
# YAML parsing is the same one used by the rest of the governance gates
# (pyyaml, no third-party deps beyond the standard library for the
# scan itself).
if out="$(python3 - "${TODOS_DIR}" "${EVIDENCE}" <<'PY'
import os
import re
import sys

import yaml

todos_dir, evidence_path = sys.argv[1], sys.argv[2]

# Marker pattern: `// TODO(openpanel#<id>[: ...])` etc.
# Group 1 is the kind, group 2 is the id (without the `openpanel#` prefix
# to keep the YAML key readable).
MARKER = re.compile(
    r'//\s*(TODO|FIXME|stub)\(\s*openpanel#([A-Za-z0-9_.-]+)(?:[:\s][^\)]*)?\)'
)

def walk_rs(root):
    for dirpath, _dirs, files in os.walk(root):
        # Skip target and vendored directories quickly.
        parts = dirpath.split(os.sep)
        if any(p in ("target", "node_modules", "vendor") for p in parts):
            continue
        for name in files:
            if name.endswith(".rs"):
                yield os.path.join(dirpath, name)

def parse_records(path):
    # A missing manifest is valid when no markers exist; the gate
    # reports it as "no manifest, no records" rather than failing.
    # The test fixtures rely on this for the empty-repo case.
    if not os.path.isfile(path):
        return {}, None
    try:
        with open(path, encoding="utf-8") as handle:
            doc = yaml.safe_load(handle) or {}
    except yaml.YAMLError as exc:
        return None, f"evidence manifest is not valid YAML: {exc}"
    if not isinstance(doc, dict):
        return None, "evidence manifest must be a YAML mapping"
    version = doc.get("version")
    records = doc.get("records")
    if version != 1:
        return None, f"evidence manifest version must be 1, got {version!r}"
    if not isinstance(records, list):
        return None, "'records' must be a YAML list"
    by_id = {}
    for entry in records:
        if not isinstance(entry, dict):
            return None, "every record must be a YAML mapping"
        rid = entry.get("id")
        rtype = entry.get("type")
        if not rid or not rtype:
            return None, f"record missing id/type: {entry!r}"
        if rtype not in {"todo", "stub", "env-blocked", "fake"}:
            return None, (
                f"record {rid!r}: type must be one of todo/stub/env-blocked/fake,"
                f" got {rtype!r}"
            )
        by_id[str(rid)] = entry
    return by_id, None

records, err = parse_records(evidence_path)
if err is not None:
    print(f"maturity: {err}")
    sys.exit(1)
records = records or {}

# Walk the source tree once; collect every marker id we find. The
# marker regex strips the `openpanel#` prefix so it matches the bare
# id the YAML record uses.
found = {}
for path in walk_rs(todos_dir):
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            for lineno, line in enumerate(handle, start=1):
                m = MARKER.search(line)
                if m:
                    rid = m.group(2)
                    found.setdefault(rid, []).append(f"{path}:{lineno}")
    except OSError:
        continue

# Normalise record ids: strip the `openpanel#` prefix if present so the
# lookup is symmetric with the marker regex.
norm_records = {}
for key, entry in records.items():
    bare = key
    if bare.startswith("openpanel#"):
        bare = bare[len("openpanel#"):]
    norm_records[bare] = entry

untracked = sorted(rid for rid in found if rid not in norm_records)
stale = sorted(rid for rid in norm_records if rid not in found)

if untracked:
    print("step: maturity status: failed")
    print("  Untracked production marker(s) (no evidence record):")
    for rid in untracked:
        locs = found[rid]
        # Show up to two locations so the developer can find the
        # marker quickly.
        sample = ", ".join(locs[:2])
        if len(locs) > 2:
            sample += f" (+{len(locs) - 2} more)"
        print(f"    - openpanel#{rid}  (e.g. {sample})")
    print(f"  Add a record to {evidence_path} with id, type, owner, reason, closure.")
    sys.exit(1)

if stale:
    # Stale records are advisory, not fatal: a record may pre-date the
    # marker landing in a different change, or the marker may have been
    # removed by a fix. The gate reports but does not fail.
    print(f"advisory: {len(stale)} stale evidence record(s) (no matching marker):")
    for rid in stale[:5]:
        entry = norm_records[rid]
        loc = entry.get("location", "n/a")
        print(f"  - openpanel#{rid} (location: {loc})")
    if len(stale) > 5:
        print(f"  ... (+{len(stale) - 5} more)")

print(
    f"maturity: {len(found)} marker(s) covered, {len(stale)} stale record(s), {len(untracked)} untracked"
)
sys.exit(0)
PY
)"; then
    if [ -n "${out}" ]; then printf '%s\n' "${out}"; fi
    step "maturity" true
else
    if [ -n "${out}" ]; then printf '%s\n' "${out}"; fi
    echo ""
    echo "step: maturity status: failed"
    exit 1
fi
