# Add bandwidth accounting — Tasks

## 1. Testing

- [x] 1.1 Unit tests: row merge, period_start truncation.
- [ ] 1.2 Property tests: sum invariants; idempotent on_byte. (deferred)
- [ ] 1.3 Service tests with mock monitor: counter row exists
      after a byte transition; threshold dispatches at 80%. (deferred)
- [ ] 1.4 Integration: live traffic through nginx + bytes
      recorded. (deferred)
- [ ] 1.5 CLI E2E: show / rolling. (deferred)
- [ ] 1.6 Web: bandwidth page. (deferred)

## 2. Domain and Application

- [x] 2.1 Add `BandwidthReader`, `BandwidthRepository`,
      `BandwidthStorageObserver`, `BandwidthCounterRow` under
      `crates/openpanel-domain/src/bandwidth_accounting/`.
- [ ] 2.2 SQLite migration for `bandwidth_counters`. (deferred)
- [ ] 2.3 Register the module and its observer hook. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add `/bandwidth/owners/{id}` and `/bandwidth/sites/{id}`. (deferred)
- [ ] 3.2 Add `openpanel bandwidth show owner|site`. (deferred)
- [ ] 3.3 Build the bandwidth page. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: stream a fixture file through an nginx
      vhost; observe counters; threshold emits notification. (deferred)
- [x] 4.4 Archive with `openspec archive add-bandwidth-accounting`.
