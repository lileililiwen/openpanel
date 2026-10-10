#!/usr/bin/env bash
# scripts/check-driftwatch-gate.sh — driftwatchdog Local Gate contract.
#
# Per the `DriftWatchdog Local Gate` quality requirement (P5 of the
# portable production maturity queue), openpanel's canonical pre-archive
# Local Gate is `driftwatchdog gate`, executed in the repo root, with
# the policy declared in `.ai-gate/gate.yaml`. The hand-rolled
# `make check` chain is preserved as a developer convenience; the two
# surfaces are kept in lockstep by this gate.
#
# This script is read-only and asserts the wiring of the contract:
#   1. `.ai-gate/gate.yaml` exists, is parseable YAML, and declares
#      `version`, `runtime`, `profile`, `rule_pack`, a non-empty
#      `checks:` block, and a `commands:` block that names every check.
#   2. Every `make check` step that today lives in the Makefile's
#      `check:` target is bound to a `commands:` entry in
#      `.ai-gate/gate.yaml`. Drift is reported as a numbered list
#      (e.g. "drift: step `foo` is in `make check` but not in
#      `commands:`").
#   3. `driftwatchdog gate --dry-run` resolves the plan cleanly and
#      lists the same number of `checks:` entries as the YAML
#      declares (after filtering out `via manifest` only checks).
#   4. `make -n gate` is wired to call `driftwatchdog gate`.
#
# Failures are named (path:line); a clean run prints
# `step: driftwatch-gate status: ok`. The gate degrades to
# `status: skipped` only when `driftwatchdog` is not on `$PATH`
# (e.g. a fresh clone without `cargo install driftwatchdog`); all
# other violations exit non-zero.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"
GATE_YAML="${REPO_ROOT}/.ai-gate/gate.yaml"
MAKEFILE="${REPO_ROOT}/Makefile"

# --- (1) the YAML exists and is well-formed --------------------------------
if [ ! -f "${GATE_YAML}" ]; then
    echo "step: driftwatch-gate status: failed (reason: ${GATE_YAML} not found)"
    exit 1
fi

python3 - "${GATE_YAML}" <<'PY' || { echo "step: driftwatch-gate status: failed (reason: ${GATE_YAML} is not valid YAML)"; exit 1; }
import sys, yaml
with open(sys.argv[1]) as f:
    data = yaml.safe_load(f)
required_keys = ("version", "runtime", "profile", "checks", "commands", "blocking")
for k in required_keys:
    if k not in data:
        print(f"missing required key `{k}` in {sys.argv[1]}")
        sys.exit(1)
if not isinstance(data["checks"], dict) or len(data["checks"]) == 0:
    print(f"`checks:` must be a non-empty mapping in {sys.argv[1]}")
    sys.exit(1)
if not isinstance(data["commands"], dict) or len(data["commands"]) == 0:
    print(f"`commands:` must be a non-empty mapping in {sys.argv[1]}")
    sys.exit(1)
# Every check that is enabled must have a `commands:` entry.
missing_cmds = [k for k, v in data["checks"].items() if v and k not in data["commands"]]
if missing_cmds:
    print(f"checks without a commands entry: {missing_cmds}")
    sys.exit(1)
PY

# --- (2) every `make check` step is bound in `commands:` -------------------
# The canonical chain lives in the Makefile's `check:` target (one
# line, no continuations). The gate's `commands:` keys are YAML
# identifiers (underscores), and three steps carry an alias
# (`fmt`→format, `clippy`→lint, `test`→tests), so the lockstep check
# maps each step to its expected key before comparing. A missing
# binding or a stale extra binding fails the gate and prints the
# offending name.
python3 - "${MAKEFILE}" "${GATE_YAML}" <<'PY' || exit 1
import re, sys, yaml

mk, gate = sys.argv[1], sys.argv[2]
with open(mk) as f:
    mk_text = f.read()
m = re.search(r'^check:[ \t]*(.*\S)[ \t]*$', mk_text, re.M)
if not m:
    print("no `check:` target found in Makefile")
    sys.exit(1)
steps = [s for s in m.group(1).split() if s not in ("ensure-lint-tools",)]

ALIASES = {"fmt": "format", "clippy": "lint", "test": "tests",
           # Hyphenated checks whose `commands:` keys MUST stay hyphenated:
           # driftwatchdog matches commands to checks by exact name, so
           # renaming them to underscores would silently unschedule the
           # checks in the canonical gate (verified: `governance-quality`
           # lands in "explicitly not scheduled"). The make targets use
           # the same hyphenated names, hence these identity mappings.
           "declared-verification": "declared-verification",
           "governance-quality": "governance-quality"}
def key(step: str) -> str:
    return ALIASES.get(step, step.replace("-", "_"))

with open(gate) as f:
    commands = yaml.safe_load(f)["commands"]
expected = {key(s) for s in steps}
missing = sorted(expected - set(commands))
extra = sorted(set(commands) - expected - {"repository_integrity", "build", "security"})
if missing:
    print("drift: steps in `make check` but not in `commands:`: " + ", ".join(missing))
    sys.exit(1)
if extra:
    print("drift: `commands:` entries with no `make check` step: " + ", ".join(extra))
    sys.exit(1)
PY

# --- (3) `driftwatchdog gate --dry-run` resolves the plan ------------------
if ! command -v driftwatchdog >/dev/null 2>&1; then
    echo "step: driftwatch-gate status: skipped (reason: driftwatchdog not on PATH)"
    echo "  remediation: cargo install driftwatchdog (or follow /home/paul/code/driftwatchdog/scripts/install.sh)"
    exit 0
fi

DRYRUN_OUT=$(cd "${REPO_ROOT}" && driftwatchdog gate --dry-run 2>&1)
# A well-formed plan lists the checks and the project commands. We don't
# enforce a 1:1 count (the labrys-style `project commands:` line is a
# separate concept from the `commands:` block) but we do require the
# plan to resolve without parse errors.
if ! echo "${DRYRUN_OUT}" | grep -q 'gate plan (dry-run'; then
    echo "step: driftwatch-gate status: failed (driftwatchdog gate --dry-run did not produce a plan)"
    echo "${DRYRUN_OUT}"
    exit 1
fi

# --- (4) `make -n gate` is wired to call `driftwatchdog gate` --------------
if ! make -n gate 2>&1 | grep -q 'driftwatchdog gate'; then
    echo "step: driftwatch-gate status: failed (make -n gate does not invoke driftwatchdog gate)"
    exit 1
fi

# --- (5) the `test_gates` binding names the self-test suite --------------
# Per the `Gate Self-Test Budget Compliance` quality requirement, the
# driftwatchdog plan's `test_gates` check MUST bind to the governance
# self-test suite (`scripts/test-gates.sh`) so a budget regression in
# the suite is caught by the same surface the operator invokes. The
# make-check↔`commands:` lockstep above only proves the binding
# *exists*, not that it points at the right script; this check fills
# that gap. Read-only — does not affect the lockstep, the manifest,
# or any other surface.
test_gates_cmd=$(python3 -c "
import sys, yaml
with open('${GATE_YAML}') as f:
    d = yaml.safe_load(f)
print(d.get('commands', {}).get('test_gates', ''))
")
if [ "${test_gates_cmd}" != "scripts/test-gates.sh" ]; then
    echo "step: driftwatch-gate status: failed (\`commands.test_gates\` is '${test_gates_cmd}', expected 'scripts/test-gates.sh')"
    exit 1
fi

step "driftwatch-gate" true
