# Add per-site WAF rules — Tasks

## 1. Testing

- [x] 1.1 Unit tests for JSON schema validation per rule kind and
      compiler byte-stability.
- [x] 1.2 Property tests: every rule kind compiles to a snippet
      that `nginx -t` accepts; unknown fields rejected; empty
      ruleset produces an empty snippet.
- [x] 1.3 Service tests with mock nginx validator and mock
      monitoring for hit ingestion and hysteresis.
- [x] 1.4 Integration: full render → `nginx -t` → reload → drive
      traffic that matches a rule → counter increments.
- [x] 1.5 CLI E2E: `openpanel waf {rules,add,remove,enable,
      disable,test}` with a real `nginx -t` invocation.
- [x] 1.6 Web: `/sites/{id}/waf` rule list, add form (CSRF),
      test-rule button, and hit counter chart.

## 2. Domain and Application

- [x] 2.1 Implement `RuleSet` aggregate and the typed `Rule` enum
      in `crates/openpanel-domain/src/waf/`.
- [x] 2.2 Add SQLite migrations and `SqliteWafRepository`.
- [x] 2.3 Implement the pure `NginxSnippetCompiler` and a
      property test that confirms byte-stability.
- [x] 2.4 Extend the sites render pass to inline the WAF snippet
      before any `location` block, and extend the existing
      `nginx -t` + reload pipeline to roll back on failure.
- [x] 2.5 Add a hit-counter sampler that publishes to the
      monitoring time series.

## 3. Adapters and UI

- [x] 3.1 Add REST routes under `/api/v1/sites/{id}/waf`.
- [x] 3.2 Add `openpanel waf` CLI subcommands.
- [x] 3.3 Add `/sites/{id}/waf` web pages with the rule list,
      add form, and CSRF.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: add a `RateLimit` rule, drive traffic,
      observe 429, confirm the hit counter increments.
- [x] 4.4 Archive with `openspec archive 2026-08-12-add-waf-rules`.

Verification note: the host has no nginx executable. The real `nginx -t`, reload,
and 429 traffic smoke ran in the isolated `nginx:alpine` container; repository
integration tests cover rendered config placement and persisted hit increments.
