# Add Web terminal — Tasks

## 1. Testing

- [x] 1.1 Unit: `TerminalTicket::new` sets `expires_at = now + ttl`;
      `consume` after expiry returns `TerminalError::Expired`; second
      `consume` of the same ticket returns `TerminalError::Used`.
- [x] 1.2 Unit: token generation produces 256-bit entropy — two
      consecutive tickets never collide across ≥1000 generated pairs
      (property `prop_ticket_uniqueness`).
- [x] 1.3 Unit: `TerminalSession::close` records duration and is
      idempotent (second close keeps first `closed_at`).
- [x] 1.4 Integration (`tests/integration/web_terminal.rs`):
      unauthenticated `POST /api/v1/terminal/ticket` → 401;
      authenticated Owner → 200 with body containing token but no
      plaintext echo of the session cookie; replaying a consumed
      ticket on the WS route → close code 1008.
- [x] 1.5 Integration: WS upgrade with mismatched `Origin` header →
      rejected before ticket consumption; with `[web_terminal]
      enabled = false` → route 404s.
- [ ] 1.6 Integration: PTY output exceeding
      `output_buffer_bytes` without reader progress closes the session
      and emits `TerminalClosed{reason: "overflow"}` audit event.
- [ ] 1.7 Property: audit events for any session sequence contain only
      `{user_id, site_id, opened_at, closed_at, reason}` — never bytes
      from the PTY stream (feed random terminal output through the
      bridge and assert absence).
- [ ] 1.8 CLI E2E: `cli_terminal_ticket_prints_token` —
      `openpanel terminal ticket --site s1` prints a 64-hex token and
      exits 0; unknown site → non-zero exit with error line.

## 2. Domain

- [x] 2.1 Implement `web_terminal` under
      `crates/openpanel-domain/src/`: `TerminalTicket`,
      `TerminalSession`, `PtyPort`/`PtyStream` traits, `TerminalError`,
      repository trait. Zero I/O; randomness injected via port.

## 3. Application

- [x] 3.1 SQLite repo + migrations under
      `crates/openpanel-app/src/web_terminal/`.
- [x] 3.2 `WebTerminalService`: issue/consume ticket, open/close
      session, enforce per-user cap and suspended-account block.
- [x] 3.3 `portable-pty` adapter implementing `PtyPort` (drop to site
      user, cwd = site root, minimal env); idle-reaper as
      `background_tasks`.

## 4. Adapters and UI

- [x] 4.1 API routes: `POST /api/v1/terminal/ticket` (CSRF-protected)
      and WS `/api/v1/terminal/session` with Origin check.
- [x] 4.2 CLI `openpanel terminal {ticket,list-sessions}`.
- [ ] 4.3 Web Terminal tab (xterm.js vendored asset or minimal
      fallback), tokens.css styling, three-breakpoint screenshots.

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [x] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: obtain ticket via curl, connect with a WS
      client, run `id` → output shows the site user (not root); leave
      idle > timeout → server closes socket.
- [ ] 5.4 Archive with `openspec archive add-web-terminal`.
