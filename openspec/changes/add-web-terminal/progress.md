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

**Remaining (tasks unchecked):** 1.6 overflow-close behaviour (bridge
currently relies on channel backpressure; explicit byte-cap close not
wired), 1.7 audit-content property test, 1.9/4.3 web terminal page
(needs a vendored xterm.js asset — offline constraint), 5.3 live WS
smoke-test (`id` shows site user), 5.4 archive. Feature flag default:
enabled; set `[modules.web_terminal] enabled = false` to disable.
