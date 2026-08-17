# Refine monitoring with bandwidth accounting and quota thresholds — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `BandwidthWindow` math, period rotation,
      and `BandwidthObserver` hook.
- [x] 1.2 Property tests: `pct_used` is monotonic; period windows
      never overlap themselves.
- [ ] 1.3 Service tests with mock observer: bytes are recorded;
      window-close is invoked exactly once per period. (deferred)

## 2. Domain and Application

- [x] 2.1 Add `BandwidthWindow`, `BandwidthPeriod`,
      `BandwidthObserver` under
      `crates/openpanel-domain/src/monitoring/bandwidth.rs`.
- [x] 2.2 Add `BandwidthThresholdCrossed`,
      `BandwidthWindowClosed` events as typed structs.
- [x] 2.3 No SQLite impact in this change (storage lands in
      the follow-on).

## 3. Adapters and UI

- [x] 3.1 No API change in this change; events are emitted
      in-process.
- [x] 3.2 No CLI / web change.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [x] 4.3 Smoke-test: a unit test invokes the observer; no DB
      rows are touched.
- [x] 4.4 Archive with `openspec archive refine-monitoring-with-bandwidth-accounting`.
