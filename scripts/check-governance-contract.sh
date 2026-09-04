#!/usr/bin/env bash
# scripts/check-governance-contract.sh — archived governance content
# ratchet.
#
# `scripts/check-spec-drift.sh` proves only that an archived delta's
# requirement *heading* still exists in the live spec. That positive
# result does not prove the requirement text, its scenarios, or its
# executable protection survived a later edit — and governance concerns
# are exactly the ones that can be weakened while every ordinary product
# test stays green.
#
# This gate is the focused, stricter ratchet for the four governance
# capabilities (`agent-quality`, `quality`, `testing`, `architecture`):
#
#   1. Content — every manifest-listed requirement is extracted from the
#      live spec, normalized (trailing whitespace trimmed, blank runs
#      collapsed) and hashed. The digest and the scenario count MUST
#      match the reviewed manifest entry.
#   2. Provenance — the manifest's archive path MUST exist and MUST
#      still contain the named requirement, so a manifest entry can
#      never point at history that was rewritten or deleted.
#   3. Executable protection — every checker ID named by an entry MUST
#      be registered in `scripts/test-gates.sh` with BOTH a positive
#      and a negative fixture (`# checker: <id> positive` /
#      `# checker: <id> negative`).
#   4. Disposition — every archived governance requirement MUST have a
#      reviewed disposition: a manifest entry (protected), or an entry
#      in the unprotected baseline (tracked debt). A newly archived
#      governance requirement with neither fails the build.
#
# The manifest is NEVER rewritten by this gate. A requirement changes
# only through a reviewed OpenSpec change that updates the manifest and
# the live requirement together.
#
# Modes:
#   check (default)    — enforce the contract; read-only.
#   --report           — print archive/capability/requirement/scenarios/
#                        digest for every archived governance requirement
#                        so a reviewer can construct the manifest.
#   --write-baseline   — record every archived governance requirement
#                        without a manifest entry into the baseline.
#
# Environment overrides (used by the self-test in scripts/test-gates.sh):
#   GOVERNANCE_CONTRACT_REPO_ROOT      — project root
#   GOVERNANCE_CONTRACT_MANIFEST       — manifest path (root-relative)
#   GOVERNANCE_CONTRACT_BASELINE       — baseline path (root-relative)
#   GOVERNANCE_CONTRACT_SELFTEST       — gate self-test path
#   GOVERNANCE_CONTRACT_ARCHIVE_DIR    — archived changes dir
#   GOVERNANCE_CONTRACT_SPECS_DIR      — live specs dir
#   GOVERNANCE_CONTRACT_CAPABILITIES   — space-separated governance caps
#
# Prints `step: governance-contract status: ok | failed | skipped` and
# exits non-zero on failure.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="${GOVERNANCE_CONTRACT_REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "${REPO_ROOT}"

MANIFEST="${GOVERNANCE_CONTRACT_MANIFEST:-openspec/governance/manifest.yaml}"
BASELINE="${GOVERNANCE_CONTRACT_BASELINE:-openspec/governance/unprotected-baseline.txt}"
SELFTEST="${GOVERNANCE_CONTRACT_SELFTEST:-scripts/test-gates.sh}"
ARCHIVE_DIR="${GOVERNANCE_CONTRACT_ARCHIVE_DIR:-openspec/changes/archive}"
SPECS_DIR="${GOVERNANCE_CONTRACT_SPECS_DIR:-openspec/specs}"
CAPABILITIES="${GOVERNANCE_CONTRACT_CAPABILITIES:-agent-quality quality testing architecture}"

mode="${1:-check}"
case "${mode}" in
    check | --report | --write-baseline) ;;
    *)
        echo "usage: scripts/check-governance-contract.sh [check|--report|--write-baseline]" >&2
        exit 2
        ;;
esac

if ! command -v python3 >/dev/null 2>&1; then
    echo ""
    echo "step: governance-contract status: skipped (python3 not installed)"
    exit 0
fi

if out="$(python3 - "${mode}" "${MANIFEST}" "${BASELINE}" "${SELFTEST}" \
    "${ARCHIVE_DIR}" "${SPECS_DIR}" ${CAPABILITIES} <<'PY'
import hashlib
import os
import re
import sys

import yaml

mode, manifest_path, baseline_path, selftest_path, archive_dir, specs_dir = sys.argv[1:7]
capabilities = sys.argv[7:]

REQ_HEAD = re.compile(r"^### Requirement:\s*(.+?)\s*$")
SCENARIO = "#### Scenario:"

failures = []
advisories = []


def blocks(path):
    """Map requirement name -> raw block text (heading line included)."""
    if not os.path.isfile(path):
        return {}
    found = {}
    current = None
    buf = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            match = REQ_HEAD.match(line)
            if match:
                if current is not None:
                    found[current] = "".join(buf)
                current = match.group(1)
                buf = [line]
                continue
            if line.startswith("### ") or line.startswith("## "):
                if current is not None:
                    found[current] = "".join(buf)
                current = None
                buf = []
                continue
            if current is not None:
                buf.append(line)
    if current is not None:
        found[current] = "".join(buf)
    return found


def normalize(text):
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    collapsed = []
    for line in lines:
        if not line.strip() and collapsed and not collapsed[-1].strip():
            continue
        collapsed.append(line)
    return "\n".join(collapsed) + "\n"


def digest(text):
    return "sha256:" + hashlib.sha256(normalize(text).encode("utf-8")).hexdigest()


def scenario_count(text):
    return sum(1 for line in text.split("\n") if line.startswith(SCENARIO))


def named(capability, requirement, archive):
    return "{0} '{1}' (archive: {2})".format(capability, requirement, archive)


# --- discover every archived governance requirement --------------------
# (archive relative path, capability, requirement) -> source block text
discovered = {}
if os.path.isdir(archive_dir):
    for change in sorted(os.listdir(archive_dir)):
        for capability in capabilities:
            delta = os.path.join(archive_dir, change, "specs", capability, "spec.md")
            if not os.path.isfile(delta):
                continue
            for requirement, body in blocks(delta).items():
                discovered[(delta, capability, requirement)] = body

# --- load the reviewed manifest ----------------------------------------
entries = []
if mode != "--report":
    if not os.path.isfile(manifest_path):
        failures.append(
            "manifest not found: {0} (run scripts/check-governance-contract.sh --report"
            " and add a reviewed entry)".format(manifest_path)
        )
    else:
        with open(manifest_path, encoding="utf-8") as handle:
            manifest = yaml.safe_load(handle) or {}
        if not isinstance(manifest, dict) or not isinstance(manifest.get("requirements"), list):
            failures.append("manifest {0}: must be a mapping with a 'requirements' list".format(manifest_path))
        else:
            for index, entry in enumerate(manifest["requirements"]):
                if not isinstance(entry, dict):
                    failures.append("manifest entry #{0}: must be a mapping".format(index))
                    continue
                missing = [
                    key
                    for key in ("archive", "capability", "requirement", "digest", "scenarios", "checkers")
                    if entry.get(key) in (None, "")
                ]
                if missing:
                    failures.append(
                        "manifest entry #{0}: missing field(s) {1}".format(index, ", ".join(missing))
                    )
                    continue
                checkers = entry["checkers"]
                if not isinstance(checkers, list) or not checkers:
                    failures.append(
                        "manifest entry #{0} ({1}): 'checkers' must be a non-empty list".format(
                            index, entry.get("requirement")
                        )
                    )
                    continue
                entries.append(entry)

# --- checker registry: every ID needs positive AND negative fixtures ---
registered = set()
if mode != "--report":
    if not os.path.isfile(selftest_path):
        failures.append("gate self-test not found: {0} (checker registry unavailable)".format(selftest_path))
    else:
        with open(selftest_path, encoding="utf-8") as handle:
            for line in handle:
                match = re.match(r"^\s*#\s*checker:\s*([A-Za-z0-9._-]+)\s+(positive|negative)\s*$", line)
                if match:
                    registered.add((match.group(1), match.group(2)))

# --- verify every manifest entry ---------------------------------------
covered = set()
for entry in entries:
    archive = entry["archive"]
    capability = entry["capability"]
    requirement = entry["requirement"]
    label = named(capability, requirement, archive)

    if capability not in capabilities:
        failures.append("{0}: capability is outside the governance scope".format(label))
        continue

    key = (archive, capability, requirement)
    if key not in discovered:
        if not os.path.isfile(archive):
            failures.append("{0}: unknown archive path".format(label))
        else:
            failures.append("{0}: requirement is not in the archived delta".format(label))
        continue
    covered.add(key)

    if not os.path.isdir(os.path.join(specs_dir, capability)):
        failures.append("{0}: live spec for capability is missing".format(label))
        continue
    live = blocks(os.path.join(specs_dir, capability, "spec.md")).get(requirement)
    if live is None:
        failures.append("{0}: requirement is missing from the live spec".format(label))
        continue

    actual_digest = digest(live)
    if actual_digest != entry["digest"]:
        failures.append(
            "{0}: requirement text changed without a reviewed manifest update"
            " (expected {1}, found {2})".format(label, entry["digest"], actual_digest)
        )
        continue

    actual_scenarios = scenario_count(live)
    if actual_scenarios != int(entry["scenarios"]):
        failures.append(
            "{0}: scenario count changed (expected {1}, found {2})".format(
                label, entry["scenarios"], actual_scenarios
            )
        )
        continue

    for checker in entry["checkers"]:
        for kind in ("positive", "negative"):
            if (checker, kind) not in registered:
                failures.append(
                    "{0}: orphaned checker id '{1}' — no {2} fixture registered in {3}".format(
                        label, checker, kind, selftest_path
                    )
                )

# --- disposition: every archived requirement must be reviewed ----------
unprotected = [key for key in sorted(discovered) if key not in covered]
if mode == "--report":
    for archive, capability, requirement in sorted(discovered):
        body = blocks(os.path.join(specs_dir, capability, "spec.md")).get(requirement) or discovered[
            (archive, capability, requirement)
        ]
        state = "protected" if (archive, capability, requirement) in covered else "unprotected"
        print(
            "{0}\t{1}\t{2}\t{3}\t{4}\t{5}".format(
                archive, capability, requirement, scenario_count(body), digest(body), state
            )
        )
    sys.exit(0)

if mode == "--write-baseline":
    with open(baseline_path, "w", encoding="utf-8") as handle:
        for archive, capability, requirement in unprotected:
            handle.write("{0}\t{1}\t{2}\n".format(archive, capability, requirement))
    print(
        "check-governance-contract: baseline written ({0} entries, {1} protected)".format(
            len(unprotected), len(covered)
        )
    )
    sys.exit(0)

baseline = set()
if os.path.isfile(baseline_path):
    with open(baseline_path, encoding="utf-8") as handle:
        for line in handle:
            parts = line.rstrip("\n").split("\t")
            if len(parts) == 3:
                baseline.add(tuple(parts))

for key in unprotected:
    archive, capability, requirement = key
    label = named(capability, requirement, archive)
    if key in baseline:
        advisories.append(
            "{0}: no executable protection (tracked in {1})".format(label, baseline_path)
        )
    else:
        failures.append(
            "{0}: archived governance requirement has no reviewed disposition — add a"
            " manifest entry with a checker or record it in {1}".format(label, baseline_path)
        )

for advisory in advisories:
    print("advisory: {0}".format(advisory))
for failure in failures:
    print("governance-contract: {0}".format(failure))

print(
    "check-governance-contract: {0} protected, {1} unprotected, {2} failure(s)".format(
        len(covered), len(unprotected), len(failures)
    )
)
sys.exit(1 if failures else 0)
PY
)"; then
    if [ -n "${out}" ]; then printf '%s\n' "${out}"; fi
    step "governance-contract" true
else
    if [ -n "${out}" ]; then printf '%s\n' "${out}"; fi
    echo ""
    echo "step: governance-contract status: failed"
    exit 1
fi
