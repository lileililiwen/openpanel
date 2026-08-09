## 1. Testing

- [x] 1.1 Unit-test all public source/cursor/parser/redactor/aggregate functions with valid, malformed, rotated, multiline, control-character, IPv4/IPv6, and secret-bearing inputs.
- [x] 1.2 Property-test arbitrary lines never panic, cursors cannot encode paths, redaction removes sensitive fields, and aggregation is idempotent.
- [x] 1.3 Service-test RBAC before file access, bounds, rotation, truncation, retention, and audit-download behavior with filesystem/repository mocks.
- [x] 1.4 Integration-test every REST route and web view for filtering, pagination, escaping, CSRF where applicable, ownership, and unauthenticated access.
- [x] 1.5 CLI E2E-test sources/tail/errors/traffic/audit/export against sandbox logs.

## 2. Implementation

- [x] 2.1 Implement log domain types, source/read/aggregate ports, redaction, cursor, and errors.
- [x] 2.2 Add safe filesystem reader, managed nginx format, parser, aggregation migration/repository, retention task, and services.
- [x] 2.3 Add REST, CLI, `/logs` web pages, HTMX polling, downloads, and navigation registration.

## 3. Validation

- [x] 3.1 Run `cargo test --workspace` twice plus large/rotating-log stress tests.
- [x] 3.2 Run `make check`, inspect responses for synthetic secrets, smoke-test tail/traffic/audit, and archive with OpenSpec.
