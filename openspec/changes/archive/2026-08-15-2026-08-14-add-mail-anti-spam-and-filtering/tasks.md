# Add Mail anti-spam and filtering — Tasks

## 1. Testing

- [x] 1.1 Unit: spam score threshold routing (folder vs inbox); Sieve
      compile success/failure; autoresponder active-window check.
- [x] 1.2 Property: spam audit event never contains message body;
      Sieve script size cap enforced; forwarder loop detection.
- [x] 1.3 Service: antispam policy apply; Sieve filter apply; greylist
      defer on first-seen; autoresponder window.
- [x] 1.4 Integration: live inbound mail routes to Spam folder above
      threshold; oversize Sieve rejected with syntax/cap error.
- [ ] 1.5 CLI E2E: `openpanel mail antispam set` -> `filter put` ->
      `autoresponder on`.
- [ ] 1.6 Web: Mail tabs (CSRF), Sieve editor, autoresponder form,
      forwarders/catch-all/list management.

## 2. Domain and Application

- [x] 2.1 Implement `AntiSpamPolicy`, `GreylistEntry`, `SieveScript`,
      `AutoResponder`, `Forwarder`, `CatchAll`, `MailingList` under
      `crates/openpanel-domain/src/mail_filtering/`.
- [x] 2.2 Add SQLite migrations for the new mail-filter tables.
- [x] 2.3 Implement `MailFilterService`, `SieveCompiler`,
      `SpamScorer`, `MailingListService`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/mail/...` REST routes (antispam, filters,
      autoresponder, forwarders, catchall, lists).
- [ ] 3.2 Add `openpanel mail {antispam,filter,autoresponder,
      forwarder,list}` CLI commands.
- [ ] 3.3 Build the Mail tabs (CSRF): Sieve editor, autoresponder form,
      forwarders, catch-all, list administration.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: enable greylisting, send a high-score probe,
      confirm it lands in Spam; oversize Sieve rejected.
- [x] 4.4 Archive with `openspec archive add-mail-anti-spam-and-filtering`.
