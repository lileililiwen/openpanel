# Progress — refine-mail-with-user-surfaces

**Goal:** expose the already-built mail_filtering domain/app layers
(sieve, autoresponder, forwarders, catch-all, mailing lists) through
REST + CLI, and replace the hardcoded queue stub with a real MTA
snapshot.

**Approach:** spec repair first (archived anti-spam delta folded into
live mail spec); `MailQueueSnapshot` + `MtaQueuePort` in domain;
`PostfixQueueAdapter` (`postqueue -j`, bounded read, JSON-lines parse,
degrade-to-unknown) + `NullQueueAdapter` for fake compositions;
`MailService::status`/new `queue_snapshot()` consume the port; REST
routes merged under `/mail` (separate sub-router state tuple); CLI
`mail {queue,filter,autoresponder}`; MailFilteringModule composed into
serve + test-support.

**Done:** 3 adapter unit tests (parse/aggregate/degrade), 5
integration tests (sieve round-trip + oversize + compile gate,
autoresponder window validation + disable, forwarder loop rejection +
round-trip, lists + catch-all surfaces, queue endpoint shape without
MTA), CLI E2E (`mail queue`). fmt/clippy clean; full workspace suite
passes.

**Update (session 2026-08-26):** task 1.6 done — `mail autoresponder`
gained a show mode (omit `--body`) and E2E set/show/disable coverage.
Task 1.8 done — `prop_surface_and_audit_never_leak` (100 fuzz cases)
proves oversize Sieve scripts are rejected pre-persistence and audit
transcripts never carry script fragments or autoresponder bodies.

**Remaining (tasks unchecked):** 1.7 web Mail tabs (Sieve editor,
autoresponder form, forwarder/catch-all/list tables — needs UI work),
5.3 live-MTA smoke-test, 5.4 archive.
