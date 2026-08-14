#!/usr/bin/env bash
# scripts/install-lint-tools.sh — install the optional lint tools
# used by `make check`'s file-length step.
#
# Tools:
#   - cargo-lint-extra   https://github.com/.../cargo-lint-extra
#   - splitrs            https://github.com/.../splitrs
#
# Both are installed via `cargo install --locked`. If install
# fails (offline / sandbox / not on crates.io yet), the function
# prints a non-fatal warning so the calling Makefile can decide
# whether to skip the step.
set -euo pipefail

log()  { printf '[INFO] %s\n' "$*"; }
warn() { printf '[WARN] %s\n' "$*" >&2; }
ok()   { printf '[OK] %s\n'   "$*"; }

install_tool() {
    local crate="$1"
    if command -v "${crate}" >/dev/null 2>&1; then
        ok "${crate} already installed: $($(command -v "${crate}") --version 2>/dev/null || echo unknown)"
        return 0
    fi
    log "installing ${crate} via cargo install --locked"
    if cargo install --locked "${crate}"; then
        ok "${crate} installed"
    else
        warn "${crate} install failed; the file-length step will skip"
        return 1
    fi
}

main() {
    local failed=0
    if ! install_tool cargo-lint-extra; then failed=1; fi
    if ! install_tool splitrs; then failed=1; fi
    return "${failed}"
}

main "$@"
