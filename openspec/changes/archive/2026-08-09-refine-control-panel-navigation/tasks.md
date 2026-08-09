## 1. Testing

- [x] 1.1 Unit-test navigation grouping, active state, empty-group omission, role filtering, breadcrumbs, and mobile disclosure markup with concrete route/role matrices.
- [x] 1.2 Unit-test settings rendering and redaction against config values containing representative URLs, passwords, tokens, and master keys.
- [x] 1.3 Integration-test every existing web route keeps its URL, unauthenticated requests redirect, User cannot access owner settings, and Owner can update each allowlisted preference with CSRF.
- [x] 1.4 Integration-test invalid timezone, unknown fields, failed atomic persistence, and audit metadata without values.

## 2. Implementation

- [x] 2.1 Replace `NAV_LINKS` with typed navigation descriptors and share registered-capability data with router composition.
- [x] 2.2 Render grouped/filtered navigation, active states, breadcrumbs, titles, and responsive CSS while preserving existing URLs.
- [x] 2.3 Implement the redacted settings read model and Owner-only GET/POST handlers with schema validation, atomic persistence, CSRF, and audit.

## 3. Validation

- [x] 3.1 Run web unit/integration tests, then `cargo test --workspace` twice.
- [x] 3.2 Run `make check` and manually verify keyboard and 375px-wide navigation.
- [x] 3.3 Archive the change with OpenSpec after all tasks pass.
