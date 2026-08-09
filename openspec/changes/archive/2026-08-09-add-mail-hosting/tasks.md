## 1. Testing

- [x] 1.1 Unit-test every public domain/address/mailbox/quota/alias/readiness constructor and transition, including Unicode rejection/normalization, quota bounds, and deletion tokens.
- [x] 1.2 Property-test alias graphs remain acyclic, address canonicalization is stable, relay policy never permits unauthenticated third-party relay, and secrets never serialize.
- [x] 1.3 Service-test readiness, config validate/apply/rollback, password one-time return, DKIM custody, quotas, rate limits, RBAC, deletion, diagnostics, and backup hooks with mocks.
- [x] 1.4 Integration-test every REST/web route for CSRF, ownership, dependency failures, secret-free responses, DNS/TLS state, and destructive confirmation.
- [x] 1.5 CLI E2E-test readiness/domain/mailbox/alias/quota/password/status commands using fake MTA/IMAP adapters.
- [x] 1.6 Add disposable Postfix/Dovecot protocol tests proving authenticated submission/IMAP works and unauthenticated relay fails.

## 2. Domain and Application

- [x] 2.1 Implement mail domain types, aggregates, repositories, MTA/IMAP/DNS/TLS/service/backup ports, and migrations.
- [x] 2.2 Implement password/DKIM custody, virtual storage, alias graph, quotas/rates, readiness, diagnostics, and audited services.
- [x] 2.3 Implement isolated Postfix/Dovecot config adapters with validation, atomic apply, reload/readiness, and rollback.

## 3. Surfaces and Validation

- [x] 3.1 Add REST, CLI, `/mail` UI, dependency setup flow, and navigation registration.
- [x] 3.2 Run `cargo test --workspace` twice, protocol/relay/backup-restore tests, and `make check`.
- [x] 3.3 Conduct an abuse/security review, smoke-test domain -> mailbox -> authenticated delivery -> backup, and archive with OpenSpec.

The live protocol contract is opt-in because it requires a disposable host with
Postfix/Dovecot and test credentials; run `cargo test --test mail_protocol -- --ignored`
with the environment documented in that test. The default suite compiles the
contract and exercises the same relay/configuration invariants with deterministic
adapters.
