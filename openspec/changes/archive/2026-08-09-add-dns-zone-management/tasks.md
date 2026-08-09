## 1. Testing

- [x] 1.1 Unit-test every public DNS name, TTL, provider, zone, and record constructor plus type-specific normalization and CNAME invariants.
- [x] 1.2 Property-test record parse/render round trips, credential ciphertext differs by nonce, arbitrary provider errors are redacted, and DNS-01 cleanup targets one ID.
- [x] 1.3 Service-test sync/import/drift/conflict, provider capabilities, credential rotation, propagation, RBAC, site proposals, and temporary leases with provider/crypto/audit mocks.
- [x] 1.4 Integration-test every REST/web route for CSRF, authorization, validation, conflicts, unavailable providers, and secret-free output.
- [x] 1.5 CLI E2E-test provider/zone/record/sync/check commands against a deterministic fake provider.

## 2. Implementation

- [x] 2.1 Implement DNS domain types, repository/provider/resolver/crypto ports, and migrations.
- [x] 2.2 Implement encrypted provider accounts, one initial provider adapter, sync/conflict logic, propagation checks, proposals, and TXT leases.
- [x] 2.3 Add REST, CLI, `/dns` pages, navigation registration, and site/SSL integration hooks.

## 3. Validation

- [x] 3.1 Run `cargo test --workspace` twice and provider contract tests against a disposable zone where credentials are available.
- [x] 3.2 Run `make check`, verify secret scans and conflict behavior, smoke-test sync -> add -> check -> delete, and archive with OpenSpec.
