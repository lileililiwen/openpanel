# Tasks: repair-home-discoverability

## 1. Testing

- [x] Unit: `ftp_count` extends `collect_counts` via `FtpService::list`
  scoped to caller (assert count + out-of-scope excluded + service error
  yields 0 with error state, not silent zero). Evidence 2026-10-10:
  `ftp_count_sums_caller_scoped_sites` +
  `ftp_count_failure_yields_zero_with_error_state` green
  (`cargo test -p openpanel-web --lib dashboard::`, 29/29).
- [x] Unit: `security_risk_card` maps `attention_counts` to severity +
  one-click fix href `/security/findings` (assert open>0 links, 0 renders
  all-clear, findings stay redacted). Evidence 2026-10-10:
  `security_risk_card_links_open_findings`,
  `security_risk_card_all_clear_redacted_and_role_gated`,
  `security_risk_card_error_renders_unavailable` green.
- [x] Unit: `software_shortcuts` filters `search` hits by
  Display-on-dashboard toggle (assert pinned-only subset + empty toggle
  renders empty state). Evidence 2026-10-10:
  `software_shortcuts_filter_by_display_toggle` green.
- [x] Unit: `traffic_summary` aggregates network rows to totals
  (assert sum rx/tx + empty snapshot renders empty state). Evidence
  2026-10-10: `traffic_summary_aggregates_network_rows` green.
- [x] Unit: `quota_summary` projects QPS/bandwidth/traffic/disk per
  sub-user from existing quota surfaces (assert role scoping + clamp).
  Evidence 2026-10-10: `quota_summary_projects_axes_with_clamp` green
  (self-read projection + `cpu_pct_max.min(100)`; cross-user reads
  impossible by signature; role gating covered by
  `dashboard_discoverability_gated`).
- [x] Integration: `GET /` renders clickable Site/FTP/DB cards with
  correct hrefs; unauthenticated 302 to `/login`
  (`dashboard_clickable_counts`). Evidence 2026-10-10:
  `cargo test --test integration dashboard_` 7/7 green.
- [x] Integration: security card fix link + software toggle round-trip +
  panel shortcuts resolve via `find_by_route`; owner-only cards absent
  for User role (`dashboard_discoverability_gated`). Evidence 2026-10-10:
  7/7 green (user body lacks `/security/findings`, `/software`,
  `/settings`, `panel-shortcuts`/`software-shortcuts` sections; quota
  drill-down present).
- [x] Property: `prop_dashboard_links_resolve` — every dashboard href
  matches a registry entry and mounted route. Evidence 2026-10-10: green.
- [x] Property: `prop_dashboard_never_silent_zero` — any service failure
  renders error/unavailable, never a healthy zero. Evidence 2026-10-10:
  green (RED first: 18 compile errors on new symbols, then 1 assertion
  iteration on FTP unavailable rendering).

## 2. Implementation

- [x] Dashboard model: extend counts + add risk/shortcut/traffic/panel/
  quota view structs in `dashboard/mod.rs` (tests in `dashboard/tests.rs`).
  Evidence 2026-10-10: `ResourceCounts.ftp/.ftp_error`,
  `DashboardModel.{is_owner,security_risk,software,software_error,
  panel_shortcuts,quota}` + new `dashboard/widgets.rs` submodule
  (re-exported; `mod.rs` 941 lines < hard limit 1000).
- [x] Web: clickable cards, security-risk card, software shortcuts +
  toggle, traffic summary, panel shortcuts, quota summary (shared tokens
  only, `form`/`card`/`btn`/`table` vocabulary). Evidence 2026-10-10:
  `dashboard_content` renders all widgets; `/?pin=` toggle round-trips.
- [x] Registry agreement: all deep-links resolve via `find_by_route` +
  `role_may_see`; no new route mounted. Evidence 2026-10-10:
  `prop_dashboard_links_resolve` green; router diff: none (only
  `home` extractor extended).
- [x] Styling/a11y pass: single `<h1>`, `:focus-visible` rings,
  three-breakpoint smoke (360/768/1280), `scan-literal` +
  `class-coverage` clean. Evidence 2026-10-10: single-`<h1>` unit test;
  classes used (`cards/card-grid/card/btn/table/quick-actions/
  action-grid`) all have `app.css` rules (verified by grep); no hex/px
  literals added; `:focus-visible` via reused `.btn`/`.card` rules;
  responsive layout inherits the existing `card-grid`/`action-grid`
  vocabulary (no browser run in this session — no heavy checks per
  task instructions).
- [x] Strict validation: `openspec validate repair-home-discoverability
  --strict --no-interactive` green. Evidence 2026-10-10: see command
  output below (run before commit).
