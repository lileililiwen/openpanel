# Progress — add-web-terminal

**Goal:** audited, scoped browser terminal over WebSocket (one-time
tickets, PTY sessions as the site user, guardrails).

**Approach:** domain ticket/session state machine with injected
randomness; SQLite repo; `portable-pty` adapter (privilege drop via
`su -s <shell> -c "cd <docroot> && exec bash" -- <user>`); axum WS
bridge with bounded output channel, idle-timeout ticker, Origin check;
REST ticket endpoint + CLI `terminal ticket`.

**Done:** domain (5 unit tests incl. 1000-token uniqueness loop),
app service/repo/adapter/module + migration, REST routes wired into
router + serve + test-support, CLI command + E2E test, 3 integration
tests (owner-only issuance w/ hex token shape, single-use consumption
across attempts incl. unknown-ticket rejection, WS route: cross-origin
upgrade → 403 before consumption, plain GET never reaches bridge).
fmt/clippy clean; full workspace suite passes.

**Update (session 2026-08-26):** task 1.6 done — bridge tracks unread
PTY output against `[web_terminal] output_buffer_bytes` (default
512 KiB) and force-closes with reason `overflow`; new `TerminalClosed`
audit action carries only session coordinates. Also fixed: interval's
immediate first tick made fresh sessions close as `idle_timeout`.
Task 1.7 done — WS mini-client integration + property prove audit
events stay coordinate-only across fuzzed session sequences.
Task 1.8 verified already covered by
`cli_terminal_ticket_prints_64_hex_token`.

**Remaining (tasks unchecked):** 1.9/4.3 web terminal page (needs a
vendored xterm.js asset — offline constraint), 5.3 live WS smoke-test
(`id` shows site user), 5.4 archive. Feature flag default: enabled;
set `[modules.web_terminal] enabled = false` to disable.
