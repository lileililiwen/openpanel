#!/usr/bin/env bash
# scripts/clean-tests.sh — delete orphaned test SQLite files and
# sandbox directories left behind by crashed or killed tests.
#
# `TestDb::new()` creates a fresh SQLite file under
# `/tmp/openpanel-test/<uuid>.db` and `TestServer` uses
# `/tmp/openpanel-cli-test-<uuid>/` for sandbox paths. Both are removed
# on `Drop` of the fixture, but a SIGKILL or `cargo test` panic can
# leak them. This script deletes anything older than 24 h.
#
# See `openspec/changes/add-tdd-infrastructure/design.md` "Risks".
set -euo pipefail

if [[ -d /tmp/openpanel-test ]]; then
    find /tmp/openpanel-test -mindepth 1 -mtime +0 -exec rm -rf {} + 2>/dev/null || true
fi

if compgen -G "/tmp/openpanel-cli-test-*" >/dev/null; then
    find /tmp -maxdepth 1 -name 'openpanel-cli-test-*' -mtime +0 \
        -exec rm -rf {} + 2>/dev/null || true
fi

echo "clean-tests: removed orphaned test fixtures from /tmp"