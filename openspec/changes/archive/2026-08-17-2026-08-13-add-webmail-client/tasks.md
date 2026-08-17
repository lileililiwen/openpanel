# Add webmail client — Tasks

## 1. Testing

- [x] 1.1 Unit tests for token mint / expiry / restore; CSRF;
      rate-limit.
- [x] 1.2 Property tests: expired tokens cannot be reused;
      restored secret ≠ master key.
- [x] 1.3 Service tests with mock bridge.
- [x] 1.4 Integration: live webmail login (test mailbox).
- [x] 1.5 No CLI E2E (web only).
- [x] 1.6 Web: full session-gated flow (CSRF).

## 2. Domain and Application

- [x] 2.1 Implement `WebmailSessionToken`, `MailBridge` trait,
      `WebmailService`, `WebmailRenderer` stubs under
      `crates/openpanel-domain/src/webmail_client/`.
- [x] 2.2 Add SQLite migration for `webmail_session_tokens`.
- [x] 2.3 Implement the bridge for the active MTA (Dovecot
      / Stalwart).

## 3. Adapters and UI

- [x] 3.1 Mount `/webmail/*` route group in `openpanel-web`.
- [x] 3.2 Build the UI: folder list, message list, message
      view, compose, reply, forward, search.
- [x] 3.3 Add `Links: Webmail` to the active-mailbox indicator
      on the mailbox detail page.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: open `/webmail`, list folders, open a
      message, reply; CSRF enforced on mutation.
- [x] 4.4 Archive with `openspec archive add-webmail-client`.
