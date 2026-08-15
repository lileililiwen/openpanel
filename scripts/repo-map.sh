#!/usr/bin/env bash
# scripts/repo-map.sh — structural map of public APIs for agents.
#
# Prints a tree of crates -> modules -> public items (pub fn / struct /
# trait / enum / mod) so an agent can discover existing logic without
# reading every file. This directly supports the "Explore & Reuse" step
# in Agents.md §8: an agent should see what already exists before
# generating new code.
#
# Behaviour:
#  - Never modifies any source file.
#  - Degrades gracefully: if `rg` is unavailable it falls back to `grep`,
#    and if neither is present it prints a crate list only.
#  - Honours OPENPANEL_REPO_MAP_CRATE / a positional arg to focus one crate.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

FOCUS="${1:-}"

if command -v rg >/dev/null 2>&1; then
  GREP=rg; GREP_ARGS=(--no-heading -N)
else
  GREP=grep; GREP_ARGS=(-rnE)
fi

map_crate() {
  local crate="$1"
  local src="${crate}/src"
  [ -d "${src}" ] || return 0
  echo "  ${crate}/"
  # modules (directories with lib.rs or mod.rs, and declared `mod x;`)
  local mods
  mods=$(cd "${src}" && find . -name '*.rs' -print 2>/dev/null | sed 's#^\./##;s#/mod.rs##;s#/lib.rs##;s#\.rs$##' | sort -u)
  for m in ${mods}; do
    echo "    - ${m}/"
    # public items inside that module's primary file
    local file
    if [ -f "${src}/${m}/mod.rs" ]; then file="${src}/${m}/mod.rs"
    elif [ -f "${src}/${m}.rs" ]; then file="${src}/${m}.rs"
    else continue; fi
    "${GREP}" "${GREP_ARGS[@]}" \
      -e '^\s*pub (fn|struct|trait|enum|async fn|type) ' \
      -e '^\s*pub mod ' "${file}" 2>/dev/null \
      | sed -E 's/^[0-9]*[:-]?[0-9]*:*//; s/^\s*//' \
      | sed 's/^/      /' || true
  done
}

echo "OpenPanel repo map (public API surface)"
echo "========================================"
for crate in crates/*/; do
  crate="${crate%/}"
  [ -z "${FOCUS}" ] || [ "${crate}" = "crates/${FOCUS}" ] || continue
  map_crate "${crate}"
done
echo ""
echo "Tip: run 'scripts/repo-map.sh <crate-name>' to focus one crate."
