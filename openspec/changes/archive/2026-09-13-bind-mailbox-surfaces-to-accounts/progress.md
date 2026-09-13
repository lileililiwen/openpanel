# Progress: bind-mailbox-surfaces-to-accounts

## Status (2026-09-13)

- Goal: bind webmail + mailbox operations to authenticated account; remove fixed demo mailbox.
- Approach: account-bound resolver on `MailService` + webmail entry binding + safe redacted errors.
- Done: research (repo-map, validate --strict green, demo leak at `webmail.rs:47-51`), principal granted standing automatic approval for this change (no separate confirm step; recorded here per HANDOFF protocol step 4).
## Verification (2026-09-13)

- `cargo test -p openpanel-app --test mail`: 13/13 green (5 new `mailbox-surfaces` tests).
- `cargo test -p openpanel-web --lib`: 189/189 green.
- `cargo test --test integration -- mail::`: 4/4 green.
- `cargo test --test integration -- mail_surfaces webmail_client`: 10/10 green (incl. new `mailbox_surfaces_authenticated_session_bound_no_demo`).
- `openspec validate bind-mailbox-surfaces-to-accounts --strict`: green.
- `webmail@example.com` remains only in test assertions (never-demo proof); zero hits in production handlers; zero `{e:?}` provider leaks in `webmail.rs`.
- `mailbox-surfaces` referenced in 4 code/test files, so the post-archive `spec-test-drift-strict` scan will cover the new live capability.
- `make check` blockers (both pre-existing, both from committed work, `Cargo.lock` untouched by this change):
  1. `audit`: `h2 0.4.15` `RUSTSEC-2026-0258` + allowed `rustls-pemfile` warning — same as HANDOFF baseline for change 3.
  2. `spec-test-drift-strict`: `[FAIL] ssl-production-lifecycle: 12 scenario(s), no covering test` — live spec shipped by commit `7c94228` (change 3) with no referencing test; not introduced here, not fixed here (one change at a time).
- All other gates green individually: fmt, clippy, docs, file-length, scan-literal, class-coverage, tasks-testing-first, reuse-strict, layering, spec-drift (0 new), agent-governance, governance-contract (0 failures), maturity (ok), test-gates 71/71.

## Reuse

- `MailService::{domains, mailboxes, mailbox_by_address}` + `authorize_domain` ownership boundary.
- `User::{id, role, email}` + identity session extractor (`WebUser`/`AuthUser`).
- `MailQueueSnapshot::unknown()` degrade + `queue_snapshot`/`status` health views.
- `AuditOutcome::Denied` + `AuditAction::MailChanged` for denied attempts.
- `WebmailService::{mint_session, validate_session}` session lifecycle (15-min TTL).
