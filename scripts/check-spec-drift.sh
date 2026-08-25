#!/usr/bin/env bash
# check-spec-drift.sh — quality gate: every requirement introduced by an
# archived OpenSpec change delta (ADDED/MODIFIED) MUST exist in the live
# spec under openspec/specs/<cap>/spec.md.
#
# Ratchet model: known pre-existing gaps are recorded in
# openspec/specs/.drift-baseline (one "<cap><TAB><requirement>" per
# line). Baseline entries are reported but do NOT fail the build;
# any NEW drift fails immediately. Shrink the baseline by merging
# deltas into live specs, then regenerate with:
#
#   scripts/check-spec-drift.sh --write-baseline
set -u

root="$(cd "$(dirname "$0")/.." && pwd)"
baseline="$root/openspec/specs/.drift-baseline"
mode="${1:-check}"

missing_file=$(mktemp)
trap 'rm -f "$missing_file"' EXIT

for delta in "$root"/openspec/changes/archive/*/specs/*/spec.md; do
  [ -f "$delta" ] || continue
  cap=$(basename "$(dirname "$delta")")
  live="$root/openspec/specs/$cap/spec.md"
  rel=${delta#"$root"/}

  if [ ! -f "$live" ]; then
    printf '%s\t%s\t%s\n' "$cap" "(entire capability missing)" "$rel" >> "$missing_file"
    continue
  fi

  names=$(awk '
    /^## (ADDED|MODIFIED) Requirements/ { inblock = 1; next }
    /^## /                              { inblock = 0 }
    inblock && /^### Requirement:/ {
      sub(/^### Requirement:[ \t]*/, "")
      print
    }
  ' "$delta")

  while IFS= read -r name; do
    [ -n "$name" ] || continue
    if ! grep -qF "### Requirement: $name" "$live"; then
      printf '%s\t%s\t%s\n' "$cap" "$name" "$rel" >> "$missing_file"
    fi
  done <<EOF2
$names
EOF2
done

if [ "$mode" = "--write-baseline" ]; then
  cut -f1,2 "$missing_file" | sort -u > "$baseline"
  echo "check-spec-drift: baseline written ($(wc -l < "$baseline") entries)"
  exit 0
fi

status=0
baseline_count=0
new_count=0
while IFS=$'\t' read -r cap name rel; do
  [ -n "${cap:-}" ] || continue
  entry="$cap	$name"
  if [ -f "$baseline" ] && grep -qFx "$entry" "$baseline"; then
    echo "drift(baseline): '$cap' missing '$name' (from $rel)"
    baseline_count=$((baseline_count + 1))
  else
    echo "DRIFT: '$cap' missing requirement '$name' (from $rel)"
    new_count=$((new_count + 1))
    status=1
  fi
done < "$missing_file"

echo "check-spec-drift: new drift: $new_count, baseline (tracked): $baseline_count"
[ "$status" -eq 0 ] && echo "check-spec-drift: OK"
exit "$status"
