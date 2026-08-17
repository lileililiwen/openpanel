# Add webmail client — Design

## Session-scoped credentials

```
WebmailSessionToken {
  user_id, mailbox,
  imap_password_ciphertext, // never plaintext
  smtp_password_ciphertext,
  expires_at = now() + 15min,
  jwt_id
}
```

The panel mints the IMAP/SMTP password by rotating the
mailbox's actual password to a short-lived token; on
token expiry or logout, the password is restored to a stored
random secret so further webmail logins are blocked.

## URL surface

```
GET    /webmail                          folder list + unread count
GET    /webmail/folder/{name}            message list with cursor
GET    /webmail/message/{id}             full message with redactions
POST   /webmail/message/{id}/reply       body: { text/html/plain }
POST   /webmail/message/{id}/forward
POST   /webmail/compose                  send via bridge
GET    /webmail/search?q=…               text search
```

All POSTs are CSRF-protected and rate-limited (token bucket
per session).

## Bridge

```rust
trait MailBridge: Send + Sync {
    async fn list_folders(&self, mb: &Mailbox) -> Vec<Folder>;
    async fn list_messages(&self, mb: &Mailbox, folder: &str, cursor: &Cursor) -> Vec<MessageHeader>;
    async fn fetch(&self, mb: &Mailbox, id: &str) -> Message;
    async fn send(&self, mb: &Mailbox, draft: &Draft) -> Result<String /* message-id */, BridgeError>;
}
```

The bridge layer is intentionally minimal so an alternative
provider (Maddy, Stalwart) can replace Dovecot without UI work.

## Tests

```
1.1  Unit: token mint + expiry + restore; CSRF middleware;
      rate-limit math.
1.2  Property: short-lived token cannot be reused after
      expiry; restored secret ≠ panel master key.
1.3  Service tests with mock bridge: list/fetch/send/compose;
      quota over-limit returns BridgeError::QuotaExceeded.
1.4  Integration: live webmail login against a test mailbox.
1.5  CLI E2E: no CLI for webmail (web only).
1.6  Web: full flow gated by panel session; CSRF on every
      mutation.
```
