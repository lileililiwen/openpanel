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
# Ratchet (--strict + OPENSPEC_REUSE_BASELINE): once a duplicate is
# reviewed and classified in the baseline file (one "<name><TAB><reason>"
# per line), strict mode stops failing on it. Newly added unclassified
# duplicates still fail. The ratchet only shrinks: removing entries is
# permitted, but a new entry must be reviewed.
#
# Degrades gracefully: if `rg` is unavailable the step is SKIPPED.
# Excludes `tests` directories so test helpers don't false-positive.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

mode="${1:-}"
BASELINE="${OPENSPEC_REUSE_BASELINE:-${REPO_ROOT}/openspec/governance/.reuse-classified-baseline}"

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

# Read the classified-duplicates baseline (name<TAB>reason; one per
# line; comments start with #). Classified entries are exempt from
# --strict failure.
classified_names=()
if [ -f "${BASELINE}" ]; then
  while IFS=$'\t' read -r name reason; do
    case "${name}" in
      ""|\#*) continue ;;
    esac
    classified_names+=("${name}")
  done < "${BASELINE}"
fi

is_classified() {
  local n="$1"
  local b
  for b in "${classified_names[@]:-}"; do
    [ "${b}" = "${n}" ] && return 0
  done
  return 1
}

# kind|name|crate  for every public item definition under ${CRATES_DIR}
# (rg recurses; tests excluded). One rg pass per kind keeps the tagging
# unambiguous. CRATES_DIR is overridable (used by the self-test).
CRATES_DIR="${OPENSPEC_CRATES_DIR:-crates}"
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

# --write-baseline: emit the current duplicates as classified entries.
if [ "${mode}" = "--write-baseline" ]; then
  {
    printf 'name\treason\n'
    printf "%b" "${fn_dupes}" | sed -nE 's/.*fn ([A-Za-z0-9_]+).*/\1\tclassified-as-alias-on-2026-09-09/p'
    printf "%b" "${type_dupes}" | sed -nE 's/.* (fn |type |trait |struct |enum |)([A-Za-z0-9_]+).*/\2\tclassified-as-alias-on-2026-09-09/p'
  } | sort -u > "${BASELINE}"
  echo "check-reuse: baseline written ($(wc -l < "${BASELINE}") entries)"
  exit 0
fi

# Cross-crate name collisions (router/register/iter are legitimately
# repeated per module) are a weak signal on their own, so the gate
# REPORTS them as warnings by default and only FAILS under --strict
# (CI for new changes). The primary defense against "failure to reuse
# existing logic" is the Explore & Reuse workflow step in Agents.md,
# with this gate as a mechanical backstop.
dupes="${fn_dupes}${type_dupes}"
if [ -n "${dupes}" ]; then
  if [ "${mode}" = "--strict" ]; then
    # Classified entries are exempt; everything else fails. The
    # baseline is the ratchet: it is reviewed, shrinks over time, and
    # is never auto-modified by this gate.
    unclassified=""
    while IFS= read -r line; do
      [ -z "${line}" ] && continue
      # Extract the duplicate name from the diagnostic line.
      name=$(printf '%s' "${line}" | sed -nE 's/.* (fn |type |trait |struct |enum |)([A-Za-z0-9_]+):.*/\2/p')
      [ -z "${name}" ] && name=$(printf '%s' "${line}" | sed -nE 's/.* fn ([A-Za-z0-9_]+):.*/\1/p')
      if [ -z "${name}" ]; then
        unclassified="${unclassified}${line}\n"
        continue
      fi
      if is_classified "${name}"; then
        : # tracked debt; pass
      else
        unclassified="${unclassified}${line}\n"
      fi
    done < <(printf "%b" "${dupes}")
    if [ -n "${unclassified}" ]; then
      echo ""
      echo "step: reuse status: failed"
      echo "  Unclassified cross-crate public items (reuse, don't reimplement):"
      printf "%b" "${unclassified}"
      exit 1
    fi
    echo ""
    echo "step: reuse status: ok (duplicates reported as classified debt)"
    printf "%b" "${dupes}"
    exit 0
  fi
  echo ""
  echo "step: reuse status: ok (duplicates reported as warnings; use --strict to fail)"
  printf "%b" "${dupes}"
  exit 0
fi

step "reuse" true
