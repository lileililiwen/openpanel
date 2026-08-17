# Add webmail client

## Why

`openspec/specs/mail/spec.md` covers mail domains, mailboxes,
aliases, and DKIM/quota refinement, but it does not include a
**webmail client**. cPanel ships Roundcube; Baota ships
RainLoop / Roundcube. Operators rarely want to ask every user
to configure a desktop mail client. This change adds a webmail
client that runs under the panel session — so authentication
is delegated and never exposes the user's mailbox password to
the panel UI surface.

## What Changes

- New bounded context `webmail-client` implementing a
  Standards-compliant IMAP/SMTP bridge.
- Roundcube-style UI rendered from `openpanel-web` (server-
  rendered, no JS framework; HTMX interactions only).
- New session-scoped credential: short-lived (15-minute) IMAP
  / SMTP password minted from a panel-managed secret.
- New URL group `/webmail/*` mounted by `openpanel-web`'s
  router.

## Capabilities

### New Capabilities

- `webmail-client`: server-rendered webmail UI and short-lived
  IMAP/SMTP bridge.

## Impact

- Domain: `WebmailSessionToken` (UUID, lifetime ≤ 15m).
- App: `WebmailService` minting credentials; `WebmailRenderer`
  for HTML; integrations with the active MTA via IMAP IDLE
  for live updates.
- Web: `/webmail/*` route group, all routes session-auth,
  CSRF on every state change.
- Security: the plaintext IMAP password is shown exactly once
  and never re-displayed.
