# Refine quality with i18n, theme-tokens, accessibility lint — Design

## Template literal scan

```sh
#!/usr/bin/env bash
# scripts/scan-template-literals.sh
set -euo pipefail
fail() { echo "scan: $*" >&2; exit 1; }

# 1. Hex colors outside tokens.css
if grep -rEn '#[0-9A-Fa-f]{3,8}\b' crates/openpanel-web/ \
    | grep -v 'tokens.css' \
    | grep -v '^.*//' \
    | grep -v '/\*' ; then
  fail "literal hex color outside tokens.css"
fi

# 2. RGB / HSL outside tokens.css
if grep -rEn '\b(rgb|hsl)\(' crates/openpanel-web/ \
    | grep -v 'tokens.css' \
    | grep -v '^.*//' ; then
  fail "literal rgb()/hsl() outside tokens.css"
fi

# 3. User-visible strings as raw HTML text in maud templates
#    (heuristic: <tag>CapitalCase</tag> on the body, not part
#    of attribute names)
if grep -rEn '>[A-Z][a-z]{2,}[ A-Za-z]*<' \
      crates/openpanel-web/src/ ; then
  fail "literal user-visible string in template body"
fi
```

## clippy.toml additions

```toml
disallowed-methods = [
  # existing entries
  { path = "std::time::SystemTime::now",          reason = "inject clock" },
  { path = "rand::random",                       reason = "inject rng" },
  # new
  { path = "format!",                            reason = "use i18n formatter" },
]
```

`format!` is not banned outright; a tighter `disallowed_types` is
added to ban locale-ignorant formatters (none implemented yet —
the i18n follow-on provides the `f!` macro). For now, format!
allows English-only strings in code paths that are not user-
visible (logs, internal panic messages, etc.).

## a11y gate

```makefile
.PHONY: a11y
a11y:
	@if curl -fs http://127.0.0.1:8080/healthz > /dev/null; then \
	  npx -y @axe-core/cli http://127.0.0.1:8080 --exit ; \
	else \
	  echo "a11y: skipped (no dev server)" ; \
	fi
```

## Tests

```
1.1  Unit: scan script rejects literal hex color in a stub
     template; accepts the same in tokens.css.
1.2  Service tests: scan script pinned to a small fixture;
     flagged for known positives and clear on negatives.
1.3  Integration: `make check` includes the scan.
```
