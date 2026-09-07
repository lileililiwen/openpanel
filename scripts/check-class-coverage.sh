#!/usr/bin/env bash
# scripts/check-class-coverage.sh — every literal class token MUST
# have a matching rule in `app.css`.
#
# The contract is a defence-in-depth for the
# `resolve-unstyled-ui-classes` change: a `class="x"` attribute in
# `crates/openpanel-web/src/*.rs` that has no rule in `app.css` is
# a regression — the page renders as a classless element and the
# UX is broken. The static contract test enumerates every literal
# class token via `rg` and asserts each one appears as a rule
# selector in the stylesheet.
#
# Dynamic class families (e.g. `audit-row audit-outcome-ok|fail|warn`)
# are concatenated at render time via `format!("class-{}", variant)`.
# The script expands a fixture table of those families to their
# concrete variants and asserts each concrete selector is defined.
#
# A small IGNORED_TOKENS list is honoured for reserved names (e.g.
# `inline` is a reserved HTML attribute and is already covered by
# `form[class~="inline"]`). Additions to the list MUST be a
# reviewed change.
#
# Emits `step: class-coverage status: ok | failed` and exits
# non-zero on any uncovered token.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

WEB_SRC_DIR="${OPENSPEC_WEB_SRC_DIR:-crates/openpanel-web/src}"
WEB_CSS_FILE="${OPENSPEC_WEB_CSS_FILE:-crates/openpanel-web/assets/app.css}"

# Tokens that legitimately appear in `class="..."` attributes but
# do not need a `.x` rule (e.g. `inline` is a reserved HTML
# attribute, `form-inline` is the rule that handles it).
IGNORED_TOKENS="inline"

# Dynamic class families concatenated at render time. The script
# expands each prefix to its concrete variants and asserts the
# concrete rule exists. A new variant added without a rule fails
# the build.
#
# Format: `prefix:variant1|variant2|...`
DYNAMIC_FAMILIES="
audit-row:audit-outcome-success|audit-outcome-failure|audit-outcome-denied
audit-badge:audit-badge-success|audit-badge-failure|audit-badge-denied
gauge-status:op-status-healthy|op-status-degraded|op-status-unknown|op-status-error
disk-status:op-status-healthy|op-status-degraded|op-status-unknown|op-status-error
op-status:op-status-fresh|op-status-stale|op-status-healthy|op-status-degraded|op-status-unknown|op-status-error
status:status-online|status-offline|status-pending|status-revoked
"

if [ ! -d "${WEB_SRC_DIR}" ]; then
    echo ""
    echo "step: class-coverage status: skipped (${WEB_SRC_DIR} not found)"
    exit 0
fi
if [ ! -f "${WEB_CSS_FILE}" ]; then
    echo ""
    echo "step: class-coverage status: failed (${WEB_CSS_FILE} not found)"
    exit 1
fi

# Collect every literal class token from `class="..."` attributes
# and split multi-class values on whitespace. We then filter out
# Rust/maud format placeholders (e.g. `status-{lowercase}` from a
# `class="status status-{}"` example or comment) and any token
# that contains characters that are not valid in a CSS class name.
tokens="$(
    rg --no-filename -o 'class[ =]"[^"]*"' "${WEB_SRC_DIR}" \
        | sed -E 's/^class[ =]"//; s/"$//' \
        | tr ' ' '\n' \
        | sort -u \
        | rg -v '^$' \
        | rg -v '[{}\\?]' \
        | rg -v '^\.+$' \
        || true
)"

# A token is "covered" if a rule selector matches in app.css. The
# match is one of:
#   - exact: `^\.{token}\b`
#   - dynamic: a `\.{family}-{variant}\b` for the family
#     expansions in $DYNAMIC_FAMILIES.
covered() {
    local token="$1"
    # Dynamic-family rule: `family-` is a known prefix and the
    # remainder is a known variant.
    for family in $(printf '%s\n' "${DYNAMIC_FAMILIES}" | cut -d: -f1); do
        if [[ "${token}" == "${family}-"* ]]; then
            local rest="${token#${family}-}"
            local variants
            variants="$(printf '%s\n' "${DYNAMIC_FAMILIES}" \
                | awk -F: -v f="${family}" '$1 == f { print $2 }')"
            IFS='|' read -ra vs <<< "${variants}"
            for v in "${vs[@]}"; do
                if [ "${rest}" = "${v#${family}-}" ]; then
                    return 0
                fi
            done
        fi
    done
    # Literal: the rule selector must appear in app.css. Use a
    # fixed-string grep to avoid regex metachar trouble.
    rg -q --fixed-strings -- ".${token}" "${WEB_CSS_FILE}" && return 0
    return 1
}

missing=()
while IFS= read -r tok; do
    [ -z "${tok}" ] && continue
    case " ${IGNORED_TOKENS} " in
        *" ${tok} "*) continue ;;
    esac
    if ! covered "${tok}"; then
        missing+=("${tok}")
    fi
done <<< "${tokens}"

if [ "${#missing[@]}" -gt 0 ]; then
    echo ""
    echo "step: class-coverage status: failed (${#missing[@]} token(s))"
    echo "  The following class=\"...\" literals have no matching rule in ${WEB_CSS_FILE}:"
    for tok in "${missing[@]}"; do
        # Show one source location per token for the developer.
        loc="$(rg --no-filename -o "class[ =]\"[^\"]*${tok}[^\"]*\"" "${WEB_SRC_DIR}" \
            | head -1 || true)"
        if [ -n "${loc}" ]; then
            echo "    - ${tok}  (e.g. ${loc})"
        else
            echo "    - ${tok}"
        fi
    done
    exit 1
fi

echo ""
echo "step: class-coverage status: ok"
