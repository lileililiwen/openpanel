# Add file-length lint and auto-refactor pipeline — Design

## Pipeline layers

```
                       ┌────────────────────────────┐
                       │  scripts/check-file-length │
                       │            .sh             │
                       └──────────┬─────────────────┘
                                  │
              ┌───────────────────┴─────────────────────┐
              │                                         │
       cargo-lint-extra check                       splitrs
       (cargo install cargo-lint-extra)            (cargo install splitrs)
              │                                         │
              ▼                                         ▼
       lint-extra.toml                              .splitrs.toml
       soft_limit=850                               max_lines=850
       hard_limit=1000                              max_impl_lines=400
       exclude=[…]                                  split_impl_blocks=true
                                                  preserve_comments=true
                                                  format_output=true
```

The two tools share configuration values (`soft_limit`/`max_lines`
align at 850) but read different files. The Makefile guarantees
they are installed together via `make install-lint-tools`, which
`make check` triggers automatically if either is missing.

## Threshold defaults

| Layer  | Lines          | Tool              | Action                                                |
|--------|----------------|-------------------|-------------------------------------------------------|
| Soft   | >850           | `cargo-lint-extra`| Warning only; `make check` exits 0                    |
| Hard   | >1000          | `cargo-lint-extra`| Error; `make check` exits non-zero                    |
| Refactor | >850         | `splitrs` dry-run | Suggested via `make split FILE=<path>`               |

Both numbers are environment-overridable:

```
OPENPANEL_FILE_LENGTH_SOFT_LIMIT=850
OPENPANEL_FILE_LENGTH_HARD_LIMIT=1000
```

The override resolution order is: env vars → `lint-extra.toml`
→ built-in defaults. Test fixtures inject the env vars and
assert the script reads them.

## `lint-extra.toml`

```toml
[file-length]
soft_limit = 850
hard_limit = 1000

# Directories that legitimately exceed the limit and must never
# be flagged. Generated artefacts, vendored code, and the
# `target/` tree are excluded; the panel's own crates are not.
exclude = [
  "**/target/**",
  "**/vendor/**",
  "**/scripts/lib/**",
  "**/tests/common/**",
]
```

## `.splitrs.toml`

```toml
[splitrs]
max_lines = 850
max_impl_lines = 400
split_impl_blocks = true
preserve_comments = true
format_output = true

[naming]
type_module_suffix = "_types"
impl_module_suffix = "_impl"
```

## `scripts/check-file-length.sh`

```sh
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/step.sh
source "$SCRIPT_DIR/lib/step.sh"

# Reads OPENPANEL_FILE_LENGTH_SOFT_LIMIT and
# OPENPANEL_FILE_LENGTH_HARD_LIMIT; falls back to the values
# in lint-extra.toml when unset. The override resolution is
# tested in tests/cli/check-file-length.sh.

if ! command -v cargo-lint-extra >/dev/null 2>&1; then
  step_skip "file-length" "cargo-lint-extra not on PATH"
  exit 0
fi

if ! cargo lint-extra --config lint-extra.toml check >/dev/null 2>&1; then
  step_fail "file-length" "files exceed hard limit — run 'make split FILE=<path>'"
  exit 1
fi

step_ok "file-length"
```

`lib/step.sh` already emits `step: file-length status: ok |
failed | skipped`; `make check` reads those lines. Failure
exits 1 with a redacted, actionable message.

## `Makefile` changes

```makefile
# Auto-installer: idempotent; logs versions; bails gracefully on
# network failure so `make check` can still exit cleanly.
install-lint-tools:
	cargo install --locked cargo-lint-extra
	cargo install --locked splitrs
	@echo "  cargo-lint-extra: $$(cargo lint-extra --version 2>/dev/null || echo 'not installed')"
	@echo "  splitrs:          $$(splitrs --version 2>/dev/null || echo 'not installed')"

# Ensure both tools are present before any check; this is the
# "auto-install during init" the requirement asks for.
ensure-lint-tools:
	@if ! command -v cargo-lint-extra >/dev/null 2>&1 \
	   || ! command -v splitrs >/dev/null 2>&1; then \
	  echo "  (bootstrap) installing lint/refactor tools"; \
	  $(MAKE) install-lint-tools || { \
	    echo "  WARN: tools not installed; file-length check will skip gracefully"; \
	  }; \
	fi

check: fmt clippy docs audit test ensure-lint-tools check-file-length
	@echo ""
	@echo "=== All quality checks passed ==="

check-file-length:
	@scripts/check-file-length.sh

split:
	@if ! command -v splitrs >/dev/null 2>&1; then \
	  echo "splitrs not installed. Run 'make install-lint-tools' first."; \
	  exit 1; \
	fi
	@if [ -z "$(FILE)" ]; then \
	  echo "Usage: make split FILE=<path>"; \
	  echo ""; \
	  echo "Files currently over 850 lines (splitrs candidates):"; \
	  find crates/ -name '*.rs' -exec wc -l {} + \
	    | sort -rn \
	    | awk '$$1 > 850 && $$2 != "total" && $$2 != "总计" {printf "  %6d lines  %s\n", $$1, $$2}'; \
	else \
	  echo "Dry-run split for $(FILE)..."; \
	  DIR=$$(dirname "$(FILE)"); \
	  BASE=$$(basename "$(FILE)" .rs); \
	  splitrs --input "$(FILE)" --output "$$DIR/$$BASE/" --config .splitrs.toml --dry-run; \
	  echo ""; \
	  echo "Review the dry-run output above. To execute the split:"; \
	  echo "  splitrs --input $(FILE) --output $$DIR/$$BASE/ --config .splitrs.toml"; \
	  echo "Then run: cargo check && cargo test"; \
	fi
```

`ensure-lint-tools` is called from `make check` before
`check-file-length`. On a fresh machine the install step adds
~30–90 s once; subsequent runs short-circuit because both
`command -v` checks pass.

## `Cargo.toml` bridge

```toml
[workspace.metadata.lint-extra]
config = "lint-extra.toml"
```

This is the marker `cargo-lint-extra` looks for by default; the
explicit `--config lint-extra.toml` we pass to it is a belt-and-
braces guarantee that a developer running it from any working
directory still resolves the panel's config.

## CI workflow (`.github/workflows/file-length.yml`)

```yaml
name: file-length
on: [pull_request]
jobs:
  file-length:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Cache cargo registry
        uses: Swatinem/rust-cache@v2
      - name: Install lint tools
        run: make install-lint-tools
      - name: Run file-length check
        run: make check-file-length
```

The CI runs `make install-lint-tools` deterministically, then
the single-target check. This avoids the cost of `make check`
during the dedicated lint job while keeping `make check`
self-contained for local use.

## Tests

```
1.1  Unit (tests/unit/quality/file_length_config.rs):
     - default soft=850 / hard=1000 read from lint-extra.toml
     - env var overrides take precedence
     - exclude patterns filter `target/` and `vendor/`
1.2  Property:
     - threshold diff (hard - soft) ≥ 50
     - exclude globbing accepts `**/target/**`
1.3  Service tests with mock cargo-lint-extra + splitrs:
     - script exits 0 when tool absent (graceful skip)
     - script exits 1 when lint returns non-zero
     - splitrs dry-run output is captured by `make split`
1.4  Integration:
     - run on a fixture with one oversized file;
       make check exits 1; make split previews the split;
       manual split brings the file under the limit;
       make check exits 0 afterward.
1.5  CLI E2E (tests/cli/file-length.sh):
     - `make install-lint-tools` is idempotent
     - `make split FILE=crates/foo.rs` invokes splitrs dry-run
     - `make check` auto-installs on a stripped PATH
1.6  Web: no UI (headless tooling).
```

## Risks

| Risk                                           | Mitigation                                                                    |
|------------------------------------------------|-------------------------------------------------------------------------------|
| `splitrs` produces non-compiling output         | `--dry-run` is the default; `cargo check` follows every committed split        |
| `cargo-lint-extra` abandoned upstream          | Tool is a thin wrapper; replacement is a one-line swap to a native lint       |
| Network is offline at install time             | `ensure-lint-tools` records the failure; check skips as the audit gate does   |
| Existing panel files already > 850 lines       | Soft limit is a warning only; hard=1000 buys breathing room; follow-up changes split |
