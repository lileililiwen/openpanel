# Tasks: Bind mailbox surfaces to accounts

## 1. Testing

- [x] Add authorization tests for owner, delegated admin, unrelated user, and disabled mailbox.
- [x] Add webmail integration tests proving the authenticated mailbox is selected and the demo mailbox is never used.
- [x] Add route tests for mailbox CRUD, forwarders, autoresponders, filters, aliases, and moderation.
- [x] Add tests for quota limits, queue depth, expired webmail sessions, and redacted failure responses.
- [x] Run the new tests red before implementation.

## 2. Implementation

- [x] Add mailbox authorization/resolution service using existing identity and collaborator boundaries.
- [x] Replace fixed webmail session minting with resolved mailbox binding.
- [x] Add missing web/API/CLI end-user mail surfaces and queue health views.
- [x] Add CSRF, audit, notification, and safe error mapping for all mutations.
- [x] Fold archived mail/mail-filtering requirements into live specs where needed.

## 3. Verification

- [x] Run focused mail, identity, webmail, and integration tests.
- [x] Run `make check` (green except two pre-existing blockers documented in `progress.md`: `audit` h2 advisory, `spec-test-drift-strict` ssl-production-lifecycle debt — both from committed work).
- [x] Run `openspec validate bind-mailbox-surfaces-to-accounts --strict`.
- [x] Verify no demo mailbox or raw provider error remains in production handlers.
