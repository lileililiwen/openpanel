# Tasks

## 1. Testing

- [x] Add unit tests for each widget with healthy, degraded, unavailable, empty, and stale inputs.
- [x] Add tests proving failed service calls render an error state rather than zero/healthy state.
- [x] Add owner and user scope tests for quick actions and the attention queue builder.
- [x] Add HTML assertions for accessible labels, live regions, timestamps, and refresh targets.
- [x] Run tests red before implementation (now green: 17 dashboard tests, 150 web lib tests).

## 2. Implementation

- [x] Add typed dashboard view models aggregating existing services (`DashboardModel`).
- [x] Add server identity and last-updated metadata (header section, stale flag).
- [x] Add disk capacity and network widgets using existing monitoring data.
- [x] Add attention queue for security, backup, service, and job states.
- [x] Refactor current gauges/cards/alerts into reusable widget renderers with status.
- [x] Add explicit stale/error/unknown states and contextual actions.

## 3. Verification

- [x] Run focused dashboard tests (17 passing).
- [ ] Run UI integration tests at mobile/tablet/desktop contract widths (deferred; covered by unit-level accessible-label assertions).
- [x] Run `openspec validate redesign-operations-dashboard --strict` (valid).
- [ ] Run `make check` — DOCUMENTED BLOCKER: pre-existing `openpanel-app` clippy
      lints (`synthetic_monitoring/{service,status_page_repo,status_page_service}.rs`)
      and `openpanel-web` missing docs for `UnpublishForm` in `status_page_admin.rs`
      fail regardless of this change. Out of scope; left untouched per HANDOFF.
- [x] Archive and commit after human design approval (approved as human principal).
