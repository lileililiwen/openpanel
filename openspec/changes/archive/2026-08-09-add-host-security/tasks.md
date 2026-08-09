## 1. Testing

- [x] 1.1 Unit-test every public rule/CIDR/port/block constructor and transition, including IPv6, ranges, protected ports, expiry, and generic responses.
- [x] 1.2 Property-test arbitrary rule input cannot escape the generated table, normalization is stable, and backoff stays bounded/monotonic.
- [x] 1.3 Service-test preview/check/apply/verify/rollback/watchdog and login throttling with mocked firewall, clock, client IP, repository, and audit ports.
- [x] 1.4 Integration-test every HTTP/web route, trusted-proxy behavior, CSRF, RBAC, restart persistence, and secret-free events.
- [x] 1.5 CLI E2E-test status/rule CRUD/preview/apply/rollback/blocks/unblock using a fake nft adapter.

## 2. Implementation

- [x] 2.1 Implement security domain, repositories, firewall/clock/client-address ports, and migrations.
- [x] 2.2 Implement isolated nftables generation and transactional safeguards plus login middleware and retention.
- [x] 2.3 Add REST, CLI, `/security` UI, posture summary, recovery documentation, and navigation registration.

## 3. Validation

- [x] 3.1 Run `cargo test --workspace` twice and namespace-isolated nftables tests where supported.
- [x] 3.2 Run `make check`, exercise rollback in a disposable environment, verify no foreign tables change, and archive with OpenSpec.
