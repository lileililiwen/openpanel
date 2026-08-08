# Tasks: Add Web UI — Dashboard

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — Dashboard

- [ ] 1.1 Unit test in `crates/openpanel-web/src/dashboard.rs`:
      given a fixed `SystemSnapshot`, the dashboard HTML contains the
      CPU / memory / disk / load gauge values.
- [ ] 1.2 Unit test: the quick-count cards render the five resource
      counts with their link targets.
- [ ] 1.3 Unit test: the alerts panel renders each alert's metric,
      value, and threshold; the empty state renders "No alerts".
- [ ] 1.4 Integration test in `tests/integration/web_ui.rs`:
      authenticated `GET /` returns the dashboard inside the shell
      (contains gauge region + card region + alerts region).
- [ ] 1.5 Integration test: `GET /dashboard/gauges` returns a partial
      with fresh CPU / memory / disk / load values.
- [ ] 1.6 Integration test: unauthenticated `GET /` redirects to
      `/login`.

## 2. Implementation — Dashboard page

- [ ] 2.1 Create `crates/openpanel-web/src/dashboard.rs`:
      `GET /` and `GET /dashboard` render the dashboard in the shell.
- [ ] 2.2 Implement the host-gauge partial (`/dashboard/gauges`) driven
      by `MonitoringService::snapshot_now`, with a 30 s HTMX refresh
      trigger on the dashboard page.
- [ ] 2.3 Implement quick-count cards using the resource services'
      list methods (sites, databases, files, ssl, users).
- [ ] 2.4 Implement the recent-alerts panel from the monitoring alert
      history, with an empty state.
- [ ] 2.5 Wire the dashboard into the shell sidebar (first nav item)
      and make `/` render it.

## 3. Validation

- [ ] 3.1 `cargo test --workspace` passes.
- [ ] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [ ] 3.3 Manual smoke: log in, confirm the dashboard shows live
      host gauges that refresh, cards, and the alerts empty state.
- [ ] 3.4 Commit + archive via OpenSpec.
