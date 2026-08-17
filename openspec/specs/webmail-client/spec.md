# webmail-client Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-webmail-client. Update Purpose after archive.
## Requirements
### Requirement: Session Credential Lifecycle

The system SHALL mint a `WebmailSessionToken` on entry to
`/webmail`, mint a short-lived (≤ 15 minute) IMAP/SMTP password
for the active mailbox, store the password encrypted in
`webmail_session_tokens`, and rotate the mailbox's actual
IMAP/SMTP password to that token. On token expiry or panel
logout, the panel MUST immediately re-rotate the mailbox
password to a panel-stored random secret and revoke any
session token that depended on the old password.

#### Scenario: Mint on entry

- **WHEN** a User navigates to `/webmail`
- **THEN** a `WebmailSessionToken` row exists with `expires_at = now + 15min` and the IMAP/SMTP password has been rotated to the token.

#### Scenario: Expiry rotates back

- **WHEN** 15 minutes pass and the user refreshes
- **THEN** the panel revokes the token, the IMAP/SMTP password is rotated back to a panel-stored random secret, and the user is redirected to re-authenticate.

#### Scenario: Logout rotates back

- **WHEN** the user logs out of the panel
- **THEN** all webmail tokens for that user are revoked and the password is rotated back.

### Requirement: Webmail UI Surface

The webmail UI SHALL be server-rendered HTML produced by
`maud` (no JS framework; HTMX allowed for progressive
interactions). Routes SHALL be session-authenticated and
mutations SHALL enforce CSRF. The minimum route surface is:
`/webmail`, `/webmail/folder/{name}`,
`/webmail/message/{id}`, `/webmail/compose`,
`/webmail/message/{id}/reply`, `/webmail/message/{id}/forward`,
`/webmail/search`.

#### Scenario: CSRF enforced on compose

- **WHEN** a client posts a compose without a valid CSRF token
- **THEN** the response is `403` and the audit `WebmailCsrfRejected` is recorded.

#### Scenario: Folder list with cursors

- **WHEN** a User opens `/webmail/folder/INBOX`
- **THEN** the response lists the most recent 50 headers with a `next_cursor` opaque token; the panel enforces a 10k message hard cap per page.

### Requirement: Quota and Sending Policy Integration

The webmail UI SHALL honour `MailboxQuota` (from the mail
refinement) and `DomainSendingPolicy`. A compose that would
exceed quota SHALL be refused at draft time with a clear
explanation; an outbound message that violates the sending
policy SHALL be refused by the bridge and surfaced as
`SendingPolicyViolation{redacted_reason}`.

#### Scenario: Over-quota compose

- **WHEN** a User composes a message that would exceed `bytes_limit` of the active mailbox
- **THEN** the compose form refuses to send with `mailbox_quota_exceeded`.

#### Scenario: Policy violated

- **WHEN** a User attempts to send with more recipients than `max_recipients_per_message`
- **THEN** the bridge returns `SendingPolicyViolation{reason="recipient_cap"}` and the message is not delivered.

### Requirement: Redaction and Secret Hygiene

The system SHALL redact inline images, hidden CSS tracking
pixels, and HTML `script` tags from the rendered message. The
plaintext IMAP/SMTP password SHALL NEVER appear in logs,
audit, API responses, or the HTML/JSON of the webmail UI. The
panel audit log MAY record `WebmailSessionCreated` with
`{mailbox, session_id}` only.

#### Scenario: Tracking pixel stripped

- **WHEN** a message body contains `<img src="https://tracker.example/p.png" width="1" height="1">`
- **THEN** the rendered HTML omits the `src` (the element remains accessible to screen-readers but does not fetch).

#### Scenario: No secret ever echoed

- **WHEN** an audit or error path would otherwise log the password
- **THEN** the entry never reaches the audit/log store; the redacted marker `[REDACTED]` is written instead.

