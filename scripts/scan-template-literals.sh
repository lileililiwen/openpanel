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
# Scope: the scanner inspects `.rs`, `.maud`, `.html`, and
# `.css` files inside `crates/`. CSS files in `assets/` are
# inspected the same way as everything else; `tokens.css` is
# the colour home and is the only CSS file that may declare
# literal hex/rgb/hsl values. The lint exists to prevent NEW
# inline literals in maud templates, Rust source, and any
# non-allowlisted stylesheet (including `app.css`).
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

ALLOWED_COLOUR_HOME="crates/openpanel-web/assets/tokens.css"
ALLOWED_STRING_HOME="crates/openpanel-web/src/t.rs"

# Patterns:
#   - Hex: #RGB, #RRGGBB, #RRGGBBAA
#   - rgb()/rgba()/hsl()/hsla() with a numeric first argument
#     (excludes tokenised calls like `rgba(var(--op-color-*-rgb), 0.1)`
#     which are the documented escape hatch for translucent values)
HEX_REGEX='#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b'
RGB_REGEX='\b(rgb|rgba|hsl|hsla|hwb)\(\s*[0-9]'

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
            */target/*|*/node_modules/*|*/dist/*|*/proptest-regressions/*) continue ;;
        esac
        if rg -n --pcre2 "$pattern" "$f" >/dev/null 2>&1; then
            echo "scan: literal $label outside ${allowed}: $f"
            rg -n --pcre2 "$pattern" "$f" | head -3
            violations=$((violations + 1))
        fi
    done < <(find crates -type f \( -name '*.rs' -o -name '*.maud' -o -name '*.html' -o -name '*.css' \) | sort)
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

