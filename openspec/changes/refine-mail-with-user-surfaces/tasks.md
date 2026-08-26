# Refine Mail with user surfaces — Tasks

## 1. Testing

- [x] 1.1 Unit: `MailQueueSnapshot` rejects `queue_depth` underflow
      (negative input via constructor returns `MailError::InvalidQueue`);
      oldest-deferred timestamp must not be in the future.
- [x] 1.2 Unit: MTA adapter parses a sample `postqueue -j` payload
      into `MailQueueSnapshot{queue_depth: 3, oldest_deferred_at: Some(_)}`;
      malformed JSON → `health = "unknown"` mapping, no panic.
- [x] 1.3 Integration (`tests/integration/mail_surfaces.rs`):
      `PUT /api/v1/mail/mailboxes/{id}/filters` with a valid script →
      200 and GET returns it; oversize script (>64 KiB) → 422
      `script_too_large`; collaborator without scope → 403.
- [x] 1.4 Integration: `PUT .../autoresponder` with
      `ends_at < starts_at` → 422 (existing window validation surfaced
      as HTTP); valid window → audit event `AutoResponderConfigured`.
- [x] 1.5 Integration: `GET /api/v1/mail/domains/{id}/queue` returns
      the mocked port's snapshot verbatim; when the port errors the
      route returns 200 with `health: "unknown"` and depth 0.
- [x] 1.6 CLI E2E: `cli_mail_autoresponder_set_and_show`,
      `cli_mail_queue_prints_depth`.
- [ ] 1.7 Web: Mail tabs render at 360/768/1280 px, forms use
      tokens.css vocabulary, Sieve editor shows byte counter; PR
      screenshots.
- [x] 1.8 Property: no surface response or audit event contains a
      Sieve script body larger than the cap or autoresponder body
      content (randomised fuzz inputs).

## 2. Spec repair (before implementation)

- [x] 2.1 Merge archived delta
      `2026-08-15-...-add-mail-anti-spam-and-filtering/specs/mail/spec.md`
      requirements into `openspec/specs/mail/spec.md`; run
      `openspec validate` on the merged spec.

## 3. Domain and Application

- [x] 3.1 Add `MailQueueSnapshot` + `MtaQueuePort` to
      `crates/openpanel-domain/src/mail_filtering/` (pure).
- [x] 3.2 Implement read-only MTA queue adapter in app layer
      (size-capped shell-out, JSON parse, degrade-to-unknown).
- [x] 3.3 Fix `MailStatus` construction to consume the port; delete
      the hardcoded `queue_depth: 0` / always-`"ready"` stubs.

## 4. Adapters and UI

- [x] 4.1 REST routes per design (DTOs, error mapping, RBAC guards).
- [x] 4.2 CLI `openpanel mail {filter,autoresponder,forwarder,catchall,list,queue}`.
- [ ] 4.3 Web Mail tabs (Sieve editor, autoresponder form,
      forwarders/catch-all/lists, queue badge).

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [x] 5.2 `make check` clean (incl. spec-test-drift gate after §2).
- [ ] 5.3 Smoke-test: set autoresponder via curl, send probe mail,
      observe single autoresponse per sender; break MTA binary path →
      queue endpoint reports unknown, panel stays up.
- [ ] 5.4 Archive with `openspec archive refine-mail-with-user-surfaces`.
