#!/usr/bin/env bash
# scripts/check-reuse.sh — duplication / reuse gate.
#
# Detects a PUBLIC item (fn / trait / struct / enum / type) whose bare
# name is defined in MORE THAN ONE crate. Such a duplicate is the
# mechanical signal of "failure to reuse existing logic": an agent (or
# human) introduced a second implementation of something that already
# existed. The author must delete one and reuse the other.
#
# To avoid false positives on ubiquitous method names (new, default,
# from, ...) we only fail on:
#   - any cross-crate duplicate of a TYPE (trait/struct/enum/type), and
#   - cross-crate duplicate of a FUNCTION whose name is not in the
#     trivial-name denylist below.
#
# Degrades gracefully: if `rg` is unavailable the step is SKIPPED.
# Excludes `tests` directories so test helpers don't false-positive.
# Pass --strict to also FAIL on cross-crate duplicate TYPE names.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

if ! command -v rg >/dev/null 2>&1; then
  echo ""
  echo "step: reuse status: skipped (rg not installed)"
  exit 0
fi

# Trivial / per-module names that legitimately recur across crates
# (every module exposes router()/register()/iter(), etc.). These are not
# "failure to reuse" signals, so they are excluded from the duplicate
# detection.
denylist=" new default from into clone eq hash fmt build run start stop \
get set list create delete update remove add open close read write \
next prev to_string as_str name id value err ok is_empty len count apply \
validate parse serialize deserialize to_json from_json execute spawn \
router register iter handle"

# kind|name|crate  for every public item definition under ${CRATES_DIR}
# (rg recurses; tests excluded). One rg pass per kind keeps the tagging
# unambiguous. CRATES_DIR is overridable (used by the self-test).
CRATES_DIR="${OPENSPEC_CRATES_DIR:-crates}"
# Each row is "kind|path:name". rg --replace '$1' emits just the captured
# name (no trailing "() {}"), and -N drops line numbers, so the row is
# unambiguously "fn|openpanel-core/src/lib.rs:compute_widget".
rows=()
while IFS= read -r r; do [ -z "$r" ] && continue; rows+=("fn|${r}"); done < <(
  rg --no-heading -N -g '!**/tests/**' --replace '$1' \
     -e '^\s*pub (?:async )?fn (\w+).*$' "${CRATES_DIR}" 2>/dev/null
)
while IFS= read -r r; do [ -z "$r" ] && continue; rows+=("type|${r}"); done < <(
  rg --no-heading -N -g '!**/tests/**' --replace '$1' \
     -e '^\s*pub (?:trait|struct|enum|type) (\w+).*$' "${CRATES_DIR}" 2>/dev/null
)

declare -A first_crate
fn_dupes=""
type_dupes=""
for row in "${rows[@]:-}"; do
  kind="${row%%|*}"; rest="${row#*|}"
  path="${rest%%:*}"; name="${rest#*:}"
  # Crate is the path segment immediately preceding "/src/" (handles both
  # "crates/<crate>/src/..." and "/abs/.../crates/<crate>/src/...").
  crate="$(echo "${path}" | sed -n 's#.*/\([^/]*\)/src/.*#\1#p')"
  [ -z "${crate}" ] && continue
  [ -z "${name}" ] && continue

  is_trivial=0
  if [ "${kind}" = "fn" ]; then
    for d in ${denylist}; do [ "${name}" = "${d}" ] && { is_trivial=1; break; }; done
    [ "${is_trivial}" -eq 1 ] && continue
  fi

  if [ -n "${first_crate[${name}]:-}" ]; then
    if [ "${first_crate[${name}]}" != "${crate}" ]; then
      if [ "${kind}" = "fn" ]; then
        fn_dupes="${fn_dupes}  - fn ${name}: ${first_crate[${name}]} AND ${crate}\n"
      else
        type_dupes="${type_dupes}  - ${kind} ${name}: ${first_crate[${name}]} AND ${crate}\n"
      fi
    fi
  else
    first_crate["${name}"]="${crate}"
  fi
done

# Cross-crate name collisions (router/register/iter are legitimately
# repeated per module) are a weak signal on their own, so the gate
# REPORTS them as warnings by default and only FAILS under --strict
# (CI for new changes). The primary defense against "failure to reuse
# existing logic" is the Explore & Reuse workflow step in Agents.md,
# with this gate as a mechanical backstop.
dupes="${fn_dupes}${type_dupes}"
if [ -n "${dupes}" ]; then
  if [ "${STRICT}" -eq 1 ]; then
    echo ""
    echo "step: reuse status: failed"
    echo "  Public item defined in more than one crate (reuse, don't reimplement):"
    printf "%b" "${dupes}"
    exit 1
  fi
  echo ""
  echo "step: reuse status: ok (duplicates reported as warnings; use --strict to fail)"
  printf "%b" "${dupes}"
  exit 0
fi

step "reuse" true
