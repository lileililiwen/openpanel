# Add Status page — Tasks

## 1. Testing

- [ ] 1.1 Unit: `derive_incidents` — consecutive Degraded results
      merge into one incident; Down→Up closes it; empty input yields
      no incidents; unresolved incident has `resolved_at = None`.
- [ ] 1.2 Unit: `uptime_bars_90d` buckets results by UTC day; a day
      with no results renders as "no data", not 100%.
- [ ] 1.3 Unit: slug generation produces base32 of 128-bit entropy;
      two generated slugs never collide across ≥1000 pairs
      (property).
- [ ] 1.4 Property: for arbitrary result sequences, derived incidents
      never overlap in time for the same check and every incident's
      `started_at` precedes its `resolved_at`.
- [ ] 1.5 Integration (`tests/integration/status_page.rs`): published
      check visible at `/status/<slug>` with label but WITHOUT its
      target URL anywhere in the HTML; unpublished check absent;
      disabled page → 404 identical to unknown slug.
- [ ] 1.6 Integration: rate limit — request 61 times in one minute →
      61st is 429 with Retry-After; Cache-Control header present on
      success responses.
- [ ] 1.7 Integration: subscribe endpoint sends confirmation via mock
      dispatcher; unsubscribed address never receives incident mail.
- [ ] 1.8 CLI E2E: `cli_status_page_enable_then_show_slug`.
- [ ] 1.9 Web: public page renders at 360/768/1280 px without
      horizontal overflow, single `<h1>`, focus-visible rings; admin
      tab screenshots in PR.

## 2. Domain

- [ ] 2.1 Add `StatusPage`, `StatusEntry`, `Incident`, slug VO, pure
      derivations under
      `crates/openpanel-domain/src/synthetic_monitoring/`.

## 3. Application

- [ ] 3.1 SQLite repo + migrations; projector from CheckResult
      history to read model.
- [ ] 3.2 `StatusPageService`: enable/disable, entry publish toggles,
      slug regeneration, subscription handling via notifications port.

## 4. Adapters and UI

- [ ] 4.1 Public route `/status/<slug>` (+subscribe) in
      openpanel-web with rate-limit middleware and cache headers.
- [ ] 4.2 Admin API routes + CLI subcommands.
- [ ] 4.3 Admin settings tab; public page template (tokens.css,
      i18n).

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean (incl. literal-scan/template gates).
- [ ] 5.3 Smoke-test: publish two checks, fail one, observe badge +
      incident bar on public page from an incognito browser.
- [ ] 5.4 Archive with `openspec archive add-status-page`.
