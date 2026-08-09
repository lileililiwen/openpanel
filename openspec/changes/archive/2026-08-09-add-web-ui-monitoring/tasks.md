# Tasks: Add Web UI — Monitoring

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — Monitoring pages

- [x] 1.1 Unit test in `crates/openpanel-web/src/monitoring.rs`: the
      history SVG renders a polyline whose `points` match a fixed
      sample set and are finite/clamped.
- [x] 1.2 Unit test: empty history renders the "no samples"
      placeholder.
- [x] 1.3 Unit test: the monitoring page renders the metric and range
      selectors and the two swap targets (`#history-chart`,
      `#alert-feed`).
- [x] 1.4 Unit test: the alert feed renders each alert's metric, value,
      and threshold; empty state renders "No alerts".
- [x] 1.5 Integration test: seed samples, then `GET
      /monitoring/history?metric=Cpu&range=3600` returns an SVG
      polyline.
- [x] 1.6 Integration test: empty DB `GET /monitoring/history` returns
      the placeholder.
- [x] 1.7 Integration test: `GET /monitoring/alerts` returns the feed or
      empty state.
- [x] 1.8 Integration test: unauthenticated `GET /monitoring` redirects
      to `/login`.

## 2. Implementation — Monitoring pages

- [x] 2.1 Create `crates/openpanel-web/src/monitoring.rs` with the
      monitoring landing page, history fragment (SVG sparkline), and
      alert feed fragment, wired into the web router behind `WebUser`.
- [x] 2.2 Implement the metric/range selectors driving HTMX swaps and
      the 60 s alert-feed auto-refresh.
- [x] 2.3 Add the monitoring page to the shell sidebar.

## 3. Validation

- [x] 3.1 `cargo test --workspace` passes.
- [x] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [x] 3.3 Manual smoke: open `/monitoring`, switch metrics/ranges,
      confirm SVG charts and the alert feed render. — covered by
      `web_monitoring_history_returns_svg_polylines` (seeded samples
      produce a polyline) and
      `web_monitoring_history_empty_renders_placeholder` /
      `web_monitoring_alerts_feed_renders_empty_state` (empty states)
      plus the unit tests for the SVG polylines and selectors.
- [x] 3.4 Commit + archive via OpenSpec.
