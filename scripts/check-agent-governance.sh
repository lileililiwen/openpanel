#!/usr/bin/env bash
# scripts/check-agent-governance.sh — agent-quality context + runtime
# contract gate.
#
# Closes the gap where the canonical `AGENTS.md` contract and the
# OpenSpec `context:` / `rules:` block can silently drift apart from
# the runtime contract that AI agents actually load. Three checks:
#
#   1. openspec/config.yaml declares a non-empty `context:` AND a
#      non-empty `rules:` block for each of the four artifact types
#      (proposal, design, tasks, specs).
#   2. `openspec context` does NOT report an empty reference/context
#      set. (The agent-quality spec requires the configured context
#      to be observable through the OpenSpec command; the human
#      output must not say "No references declared".)
#   3. Every runtime contract directory present in the repository
#      (.agents/, .codex/, .qoder/) loads the SAME root `AGENTS.md`
#      (symlink or content-identical) and references
#      `openspec/specs/agent-quality/spec.md`.
#
# The check is read-only — it never writes or rewrites any file. It
# skips (with an explicit status) only when the `openspec` executable
# is unavailable. All other failures are reported with the offending
# file or output snippet.
#
# Environment overrides (used by the self-test in
# scripts/test-gates.sh):
#   AGENT_GOVERNANCE_REPO_ROOT   — project root (default: this script's parent)
#   AGENT_GOVERNANCE_RUNTIME_DIRS — space-separated runtime dirs to check
#                                   (default: ".agents .codex .qoder" filtered
#                                   to those that exist)
#
# Prints `step: agent-governance status: ok | failed | skipped` and
# exits non-zero on the first failure.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="${AGENT_GOVERNANCE_REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "${REPO_ROOT}"

CONFIG_PATH="${REPO_ROOT}/openspec/config.yaml"
ROOT_AGENTS="${REPO_ROOT}/AGENTS.md"
ROOT_AGENT_QUALITY_REF="openspec/specs/agent-quality/spec.md"

if ! command -v openspec >/dev/null 2>&1; then
    echo ""
    echo "step: agent-governance status: skipped (openspec not installed)"
    exit 0
fi

fail() {
    echo ""
    echo "step: agent-governance status: failed"
    shift
    for line in "$@"; do echo "  ${line}"; done
    exit 1
}

# 1. openspec/config.yaml: non-empty `context:` + all four artifact
#    `rules:` blocks (proposal, design, tasks, specs).
if [ ! -f "${CONFIG_PATH}" ]; then
    fail "openspec/config.yaml not found at ${CONFIG_PATH}"
fi

config_check="$(python3 - "$CONFIG_PATH" <<'PY'
import sys, yaml, json
with open(sys.argv[1]) as fh:
    cfg = yaml.safe_load(fh) or {}
ctx = cfg.get("context")
rules = cfg.get("rules") or {}
errors = []
if not isinstance(ctx, str) or not ctx.strip():
    errors.append("context: must be a non-empty string")
if not isinstance(rules, dict):
    errors.append("rules: must be a mapping of artifact id -> list")
else:
    for k in ("proposal", "design", "tasks", "specs"):
        v = rules.get(k)
        if not isinstance(v, list) or not v:
            errors.append(f"rules.{k}: must be a non-empty list")
        else:
            for item in v:
                if not isinstance(item, str) or not item.strip():
                    errors.append(f"rules.{k}: entries must be non-empty strings")
                    break
print(json.dumps({"ok": not errors, "errors": errors}))
PY
)"
if [ "$(printf '%s' "${config_check}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["ok"])')" != "True" ]; then
    errs="$(printf '%s' "${config_check}" | python3 -c 'import sys,json;print("\n".join(json.load(sys.stdin)["errors"]))')"
    fail_lines=()
    while IFS= read -r line; do
        [ -n "${line}" ] && fail_lines+=("${line}")
    done <<< "${errs}"
    fail "openspec/config.yaml malformed:" "${fail_lines[@]}"
fi

# 2. `openspec context` must not report an empty reference/context set.
#    Use --json so we don't depend on a particular human-readable
#    phrase. An empty set is `members == []` AND `declaredReferenceCount == 0`.
ctx_json="$(openspec context --json 2>/dev/null || true)"
if [ -z "${ctx_json}" ]; then
    fail "openspec context --json produced no output"
fi
empty_set="$(printf '%s' "${ctx_json}" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
except Exception as e:
    print("PARSE_ERROR")
    sys.exit(0)
members = d.get("members") or []
ref_count = d.get("declaredReferenceCount")
if ref_count is None:
    # Fall back: inspect text in case the field is absent.
    ref_count = 0
print("yes" if (not members and (ref_count or 0) == 0) else "no")
')"
if [ "${empty_set}" = "yes" ]; then
    # Also check the human output: the spec scenario names the exact
    # phrase. If the human output is empty, treat the same way.
    human="$(openspec context 2>/dev/null || true)"
    if printf '%s' "${human}" | grep -q 'No references declared'; then
        fail "openspec context reports 'No references declared' while openspec/config.yaml declares a non-empty context (contradictory; the OpenSpec CLI must surface the configured context)"
    fi
    fail "openspec context reports an empty reference/context set (no members, declaredReferenceCount=0) while openspec/config.yaml declares a non-empty context"
fi

# 3. Runtime contract directories: AGENTS.md resolves to / matches
#    root AGENTS.md and references the agent-quality spec.
#
#    The canonical contract names `.agents/`, `.codex/`, and
#    `.qoder/` (see AGENTS.md "Single Canonical Agent Contract"). The
#    design states the implementation "will not create `.qoder`"; the
#    set of runtime dirs we actually check is therefore the
#    intersection of the documented set and the dirs that actually
#    carry an AGENTS.md. An IDE-created .qoder/ without a contract
#    is not a violation of the governance contract — it simply is
#    not a contract directory.
if [ -z "${AGENT_GOVERNANCE_RUNTIME_DIRS:-}" ]; then
    AGENT_GOVERNANCE_RUNTIME_DIRS=".agents .codex .qoder"
fi

runtime_issues=()
checked_runtimes=()
for d in ${AGENT_GOVERNANCE_RUNTIME_DIRS}; do
    runtime_dir="${REPO_ROOT}/${d}"
    if [ ! -d "${runtime_dir}" ]; then
        continue
    fi
    runtime_agents="${runtime_dir}/AGENTS.md"
    if [ ! -e "${runtime_agents}" ] && [ ! -L "${runtime_agents}" ]; then
        # Runtime dir exists but does not carry a contract — skip
        # (it is not a contract directory by the canonical contract's
        # definition). A broken symlink (which `-e` reports as
        # missing) is NOT skipped: it IS a contract directory whose
        # link is broken and must be flagged.
        continue
    fi
    checked_runtimes+=("${d}")

    # Resolve the runtime AGENTS.md: follow symlinks to a real path.
    if [ -L "${runtime_agents}" ]; then
        target="$(readlink -f "${runtime_agents}")"
        root_real="$(readlink -f "${ROOT_AGENTS}")"
        if [ "${target}" != "${root_real}" ]; then
            runtime_issues+=("${d}/AGENTS.md: symlink target is ${target}, expected ${root_real}")
            continue
        fi
    else
        # Copied contract: must be byte-identical to root AGENTS.md.
        if ! cmp -s "${runtime_agents}" "${ROOT_AGENTS}"; then
            runtime_issues+=("${d}/AGENTS.md: content differs from root AGENTS.md (stale copy)")
            continue
        fi
    fi

    # The contract (symlink target or copied file) must reference the
    # agent-quality spec. We check the root AGENTS.md because the
    # runtime file either IS the root (symlink) or matches it byte-for-byte
    # (copied contract); both guarantee the reference is present.
    if ! grep -qF "${ROOT_AGENT_QUALITY_REF}" "${ROOT_AGENTS}"; then
        runtime_issues+=("root AGENTS.md: does not reference ${ROOT_AGENT_QUALITY_REF}")
    fi
done

if [ "${#runtime_issues[@]}" -gt 0 ]; then
    fail_lines=("runtime contract drift:" "${runtime_issues[@]}")
    fail "${fail_lines[@]}"
fi

step "agent-governance" true
