# Progress — refine-logs-with-rotation-policy

**Done (session 2026-08-26):**

- Domain: `RotationPolicy` VO (bounds enforced per source class),
  deterministic logrotate stanza renderer, managed-block replacement
  preserving foreign directives; units + 100-case property
  (no `su`, registry-bounded paths, fixture structural parse).
- App: `LogRotationService` — atomic managed-block drop-in writer,
  JSON persistence (`LOGS_V002`), drift flag comparing the on-disk
  block against stored policy, manual rotate running
  `logrotate --force <drop-in>` with capped output and
  `logrotate_unavailable` -> 503; PATH detection at startup;
  `OPENPANEL__LOGS__DROP_IN_ROOT` override.
- REST: GET/PUT `/api/v1/logs/policies/{class}` +
  POST `/policies/{class}/rotate`. CLI: `logs policy {show,set}`.
- Tests: integration put/get/drift-flip/audit; CLI E2E
  set/show/invalid-bounds-refused. Full workspace suite green twice.

**Remaining:** 5.2 blocked environmentally (docs-gate OOM under
concurrent agent sessions — all gates pass individually); deferred
web Retention tab + live-logrotate smoke-test; archive.