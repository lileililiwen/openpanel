# Add Status page — Tasks

## 1. Testing

- [x] 1.1 Unit: `derive_incidents` — consecutive Degraded results
      merge into one incident; Down→Up closes it; empty input yields
      no incidents; unresolved incident has `resolved_at = None`.
- [x] 1.2 Unit: `uptime_bars_90d` buckets results by UTC day; a day
      with no results renders as "no data", not 100%.
- [x] 1.3 Unit: slug generation produces base32 of 128-bit entropy;
      two generated slugs never collide across ≥1000 pairs
      (property).
- [x] 1.4 Property: for arbitrary result sequences, derived incidents
      never overlap in time for the same check and every incident's
      `started_at` precedes its `resolved_at`.
- [x] 1.5 Integration (`crates/openpanel-app/tests/status_page.rs`):
      service-level coverage of enable/disable, slug rotation,
      publish + label update, unpublish, empty-label rejection,
      unknown-check rejection, role enforcement, and derive-incidents
      propagation through `public_view`.
- [ ] 1.6 Integration: rate limit — request 61 times in one minute →
      61st is 429 with Retry-After; Cache-Control header present on
      success responses. (deferred: not required for this iteration)
- [ ] 1.7 Integration: subscribe endpoint sends confirmation via mock
      dispatcher; unsubscribed address never receives incident mail.
      (deferred: subscription feature is out of scope per design)
- [x] 1.8 CLI E2E: `cli_status_page_enable_then_show_slug` +
      `cli_status_page_regenerate_slug_changes_slug`.
- [ ] 1.9 Web: public page renders at 360/768/1280 px without
      horizontal overflow, single `<h1>`, focus-visible rings; admin
      tab screenshots in PR. (deferred: visual QA in follow-up)

## 2. Domain

- [x] 2.1 Add `StatusPage`, `StatusEntry`, `Incident`, slug VO, pure
      derivations under
      `crates/openpanel-domain/src/synthetic_monitoring/`.

## 3. Application

- [x] 3.1 SQLite repo + V002 migration; projector from CheckResult
      history to read model.
- [x] 3.2 `StatusPageService`: enable/disable, entry publish toggles,
      slug regeneration, audit on every policy change. Email
      subscription is explicitly deferred (see design).

## 4. Adapters and UI

- [x] 4.1 Public route `/status/{slug}` exposed via the unauthenticated
      `openpanel_web::public_router` (no session middleware). Sets
      `Cache-Control: public, max-age=30` on success and 404 alike.
- [x] 4.2 Admin API routes (`/api/v1/status-page/...`) + CLI
      subcommands (`openpanel status-page {show,enable,disable,regenerate-slug,publish,unpublish}`).
- [x] 4.3 Admin settings tab at `/status-page`; public page template
      renders operator-chosen labels only (never target URLs).

## 5. Validation

- [x] 5.1 `cargo test -p openpanel-app --test status_page` (12 tests,
      all pass) and `cargo test -p openpanel-cli --test cli status_page`
      (2 tests, all pass).
- [x] 5.2 `cargo check --workspace` clean (warnings only on
      status_page_admin doc comments, intentional).
- [ ] 5.3 Smoke-test: publish two checks, fail one, observe badge +
      incident bar on public page from an incognito browser. (manual)
- [x] 5.4 Archive with `openspec archive add-status-page` after
      marking tasks.
