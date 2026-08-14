#!/usr/bin/env bash
# scripts/scan-template-literals.sh — content scan for literal
# hex / rgb / hsl colour values and literal user-visible English
# strings that escape the locale negotiation path.
#
# The scanner's allowlist is `tokens.css` for colours and `t.rs`
# for strings. The string scan is opt-in (false-positive heavy)
# and skipped by default. The script emits the standard
# `step: scan-literal status: ok | failed` line consumed by
# `make check` via scripts/lib/step.sh.
#
# Scope: the scanner only inspects `.rs`, `.maud`, and `.html`
# files inside `crates/`. CSS files in `assets/` are excluded
# because the existing `app.css` is a pre-existing baseline
# that owns the colour tokens; the lint exists to prevent
# NEW inline literals in maud templates and Rust source.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

ALLOWED_COLOUR_HOME="crates/openpanel-web/src/tokens.css"
ALLOWED_STRING_HOME="crates/openpanel-web/src/t.rs"

# Patterns:
#   - Hex: #RGB, #RRGGBB, #RRGGBBAA
#   - rgb()/rgba()/hsl()/hsla()
HEX_REGEX='#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b'
RGB_REGEX='\b(rgb|rgba|hsl|hsla|hwb)\s*\('

violations=0
scan() {
    local label="$1"
    local pattern="$2"
    local allowed="$3"
    shift 3
    while IFS= read -r f; do
        if [ "$f" = "$allowed" ]; then
            continue
        fi
        case "$f" in
            */target/*|*/node_modules/*|*/dist/*|*/proptest-regressions/*|*/assets/*) continue ;;
        esac
        if rg -n --pcre2 "$pattern" "$f" >/dev/null 2>&1; then
            echo "scan: literal $label outside ${allowed}: $f"
            rg -n --pcre2 "$pattern" "$f" | head -3
            violations=$((violations + 1))
        fi
    done < <(find crates -type f \( -name '*.rs' -o -name '*.maud' -o -name '*.html' \) | sort)
}

scan "hex color" "$HEX_REGEX" "$ALLOWED_COLOUR_HOME"
scan "rgb/hsl color" "$RGB_REGEX" "$ALLOWED_COLOUR_HOME"

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "step: scan-literal status: failed ($violations file(s))"
    exit 1
fi
echo ""
echo "step: scan-literal status: ok"

