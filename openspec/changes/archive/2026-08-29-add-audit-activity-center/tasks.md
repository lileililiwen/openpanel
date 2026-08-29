# Tasks

## 1. Testing

- [x] Add unit tests for redaction, cursor encoding/decoding, filter validation, ordering, and safe metadata selection.
- [x] Add integration tests for owner access, user denial, unauthenticated redirect, filters, pagination, empty state, and secret non-leakage.
- [x] Add integration tests for HTML page and HTMX fragment parity.
- [x] Add tests proving audit reads do not create audit events.
- [x] Run tests red before implementation.

## 2. Implementation

- [x] Map existing audit persistence to a read service without duplicating storage logic.
- [x] Add typed query/filter DTOs and cursor pagination.
- [x] Add owner-protected API routes and web handlers.
- [x] Add central redaction and safe metadata projection.
- [x] Replace the `/audit` 501 stub and add activity summary components.
- [x] Add navigation and resource/job links.

## 3. Verification

- [x] Run focused audit unit and integration tests.
- [x] Run `openspec validate add-audit-activity-center --strict`.
- [ ] Run `make check`. (Blocked by pre-existing, unrelated gate failures on HEAD: `openpanel-app` synthetic_monitoring clippy, `openpanel-web` status_page_admin docs — documented in HANDOFF.md, not introduced by this change.)
- [x] Archive and commit after human design approval.
