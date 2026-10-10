# Tasks: repair-home-discoverability

## 1. Testing

- [ ] Unit: `ftp_count` extends `collect_counts` via `FtpService::list`
  scoped to caller (assert count + out-of-scope excluded + service error
  yields 0 with error state, not silent zero).
- [ ] Unit: `security_risk_card` maps `attention_counts` to severity +
  one-click fix href `/security/findings` (assert open>0 links, 0 renders
  all-clear, findings stay redacted).
- [ ] Unit: `software_shortcuts` filters `search` hits by
  Display-on-dashboard toggle (assert pinned-only subset + empty toggle
  renders empty state).
- [ ] Unit: `traffic_summary` aggregates network rows to totals
  (assert sum rx/tx + empty snapshot renders empty state).
- [ ] Unit: `quota_summary` projects QPS/bandwidth/traffic/disk per
  sub-user from existing quota surfaces (assert role scoping + clamp).
- [ ] Integration: `GET /` renders clickable Site/FTP/DB cards with
  correct hrefs; unauthenticated 302 to `/login`
  (`dashboard_clickable_counts`).
- [ ] Integration: security card fix link + software toggle round-trip +
  panel shortcuts resolve via `find_by_route`; owner-only cards absent
  for User role (`dashboard_discoverability_gated`).
- [ ] Property: `prop_dashboard_links_resolve` — every dashboard href
  matches a registry entry and mounted route.
- [ ] Property: `prop_dashboard_never_silent_zero` — any service failure
  renders error/unavailable, never a healthy zero.

## 2. Implementation

- [ ] Dashboard model: extend counts + add risk/shortcut/traffic/panel/
  quota view structs in `dashboard/mod.rs` (tests in `dashboard/tests.rs`).
- [ ] Web: clickable cards, security-risk card, software shortcuts +
  toggle, traffic summary, panel shortcuts, quota summary (shared tokens
  only, `form`/`card`/`btn`/`table` vocabulary).
- [ ] Registry agreement: all deep-links resolve via `find_by_route` +
  `role_may_see`; no new route mounted.
- [ ] Styling/a11y pass: single `<h1>`, `:focus-visible` rings,
  three-breakpoint smoke (360/768/1280), `scan-literal` +
  `class-coverage` clean.
- [ ] Strict validation: `openspec validate repair-home-discoverability
  --strict --no-interactive` green.
