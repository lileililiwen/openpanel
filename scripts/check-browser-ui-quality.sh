#!/usr/bin/env bash
# scripts/check-browser-ui-quality.sh — rendered browser UI quality gate.
#
# Covers `browser-ui-quality`: Rendered Accessibility Gate, Responsive
# Route Gate, Typed Localization, Reduced Motion.
#
# The gate runs the deterministic core (`openpanel-web`
# `browser_ui_quality` unit tests) plus the static CSS contract probes.
# The full Playwright + axe run lives in `tests/browser/quality.mjs`
# and in `.github/workflows/browser-ui-quality.yml`: when node +
# browsers are available the script runs it against
# `OPENPANEL_BROWSER_BASE_URL`; when they are absent (local dev without
# browser deps) the browser leg reports as infrastructure-skipped so
# `make check` stays green — per design, environment startup failures
# are infrastructure failures, while real axe/accessibility failures
# block.
#
# Emits `step: browser-ui-quality status: ok | failed | skipped`.
set -euo pipefail
source "$(dirname "$0")/lib/step.sh"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

fail=0

echo ""
echo "step: browser-ui-quality status: running"

# 1. Deterministic core: unit tests for the route matrix, evaluators,
#    catalog fallback, and formatting helpers.
if cargo test -p openpanel-web --lib browser_ui_quality >/dev/null 2>&1; then
  echo "  ok   - browser_ui_quality unit tests"
else
  echo "  FAIL - browser_ui_quality unit tests"
  fail=1
fi

# 2. Stylesheet contract probes (focus ring + reduced motion).
CSS="crates/openpanel-web/assets/app.css"
if [ -f "${CSS}" ]; then
  if grep -q ':focus-visible' "${CSS}" && grep -q '\-\-op-color-focus-ring' "${CSS}"; then
    echo "  ok   - focus-visible ring present"
  else
    echo "  FAIL - focus-visible ring missing from app.css"
    fail=1
  fi
  if grep -q 'prefers-reduced-motion: reduce' "${CSS}"; then
    echo "  ok   - prefers-reduced-motion block present"
  else
    echo "  FAIL - prefers-reduced-motion block missing from app.css"
    fail=1
  fi
else
  echo "  FAIL - ${CSS} not found"
  fail=1
fi

# 3. Pinned harness manifest must exist.
if [ -f "tests/browser/package.json" ] && [ -f "tests/browser/quality.mjs" ]; then
  echo "  ok   - pinned browser harness present"
else
  echo "  FAIL - tests/browser harness missing"
  fail=1
fi

# 4. Live browser leg (optional locally, blocking in CI).
BASE_URL="${OPENPANEL_BROWSER_BASE_URL:-}"
if [ -n "${BASE_URL}" ]; then
  if command -v node >/dev/null 2>&1 && [ -d "tests/browser/node_modules" ]; then
    if (cd tests/browser && node quality.mjs --base-url "${BASE_URL}"); then
      echo "  ok   - playwright/axe run passed"
    else
      echo "  FAIL - playwright/axe run failed (see artifacts in target/browser-artifacts)"
      fail=1
    fi
  else
    echo "  FAIL - OPENPANEL_BROWSER_BASE_URL is set but node/node_modules is missing"
    fail=1
  fi
else
  echo "  skip  - live browser run (set OPENPANEL_BROWSER_BASE_URL to enable; CI always sets it)"
fi

if [ "${fail}" -ne 0 ]; then
  echo ""
  echo "step: browser-ui-quality status: failed"
  exit 1
fi
echo ""
echo "step: browser-ui-quality status: ok"
