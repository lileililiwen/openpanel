# Refine mail with DKIM / SPF / DMARC defaults and mailbox quotas — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `DkimKeypair` generation, signature,
      and rotation grace period enforcement.
- [x] 1.2 Property tests: every enabled domain has a
      `DkimKeypair` row; mailbox quota math rejects at the
      boundary; sending policy refuses unsigned and misaligned
      outbound.
- [ ] 1.3 Service tests with mock MDA and MTA: DKIM rotation
      grace, quota rejection, policy rejection, audit. (deferred)
- [ ] 1.4 Integration: enabled domain → DKIM DNS record
      suggested; mailbox over quota → MDA returns 5.2.2;
      outbound unsigned message rejected. (deferred)
- [ ] 1.5 CLI E2E. (deferred)
- [ ] 1.6 Web. (deferred)

## 2. Domain and Application

- [x] 2.1 Add `DkimKeypair`, `MailboxQuota`,
      `DomainSendingPolicy`, `SendingRequirement` value
      objects under `crates/openpanel-domain/src/mail/dkim.rs`.
- [ ] 2.2 Add SQLite migration for `dkim_keys`,
      `mailbox_quotas`, `domain_sending_policies`. (deferred)
- [ ] 2.3 Implement `DkimGenerator`, `QuotaEnforcer`,
      `SendingPolicyEnforcer` in the app crate. (deferred)
- [ ] 2.4 Wire `MailService::enable_domain` to call
      `DkimGenerator::generate` and apply the default
      sending policy. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add REST routes under `/api/v1/mail/`. (deferred)
- [ ] 3.2 Add CLI subcommands. (deferred)
- [ ] 3.3 Add the mail settings tab on the domain detail page.
      (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: enable a fresh mail domain; verify DKIM
      keypair; over-quota a mailbox in a fixture maildir;
      policy reject produces audit event. (deferred)
- [x] 4.4 Archive with `openspec archive refine-mail-with-dkim-spf-dmarc-defaults`.
