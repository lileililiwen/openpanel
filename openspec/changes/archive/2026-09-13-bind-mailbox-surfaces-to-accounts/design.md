# Design: Bind mailbox surfaces to accounts

## Approach

Use the existing identity/session extractor and mail domain repository as the
authorization boundary. Introduce a mailbox-access policy that resolves the
authenticated principal to an owned or delegated mailbox before minting a
short-lived webmail session. End-user operations remain in the mail bounded
context; webmail renders only authorized views and submits CSRF-protected
commands.

## Explore & Reuse

- Reuse `WebUser`, identity roles, collaborator grants, and session middleware.
- Reuse mail domain, mailbox, filtering, deliverability, notification, and
  audit services already exported by `openpanel-app`.
- Reuse webmail session validation and existing UI state components.
- Reuse existing route and integration-test conventions rather than adding a
  JavaScript mail client.

## Boundaries

Mail domain logic owns ownership and lifecycle; webmail owns presentation and
short-lived session handoff; identity owns authentication; notification owns
delivery alerts. No handler may select a mailbox from a fixed constant.

## Verification

Test owner/admin/user scope, delegated access, expired sessions, every mutation,
quota rejection, safe error rendering, and message-content/credential redaction.

## Non-goals

- New mail transport.
- Cross-account mailbox discovery.
- Unbounded message search.
