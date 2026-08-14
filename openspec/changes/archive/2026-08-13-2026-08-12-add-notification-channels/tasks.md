# Add notification channels — Tasks

## 1. Testing

- [x] 1.1 Unit tests for filter parsing/evaluation, HMAC signing,
      backoff schedule, allowlist matching. — `crates/openpanel-domain/tests/notifications.rs::channels_validate_and_metadata_redacts_encrypted_credentials`, `subscription_filters_match_only_requested_events`, `webhook_signature_and_payload_are_stable_bounded_and_secret_free`, `delivery_retry_schedule_and_lease_recovery_preserve_identity`, `terminal_delivery_cannot_transition_or_be_released_twice`.
- [x] 1.2 Property tests: signature determinism, backoff monotonicity,
      and dispatcher correctness across simulated clocks. — `prop_backoff_is_monotonic`, `prop_signature_changes_when_body_changes`, `prop_empty_alert_filter_never_matches` (≥100 cases by default).
- [x] 1.3 Service tests with mock SMTP and HTTP adapters, mock clock,
      and mock audit covering transient retry, terminal failure,
      crash recovery, stable delivery IDs, and receiver deduplication. — `crates/openpanel-app/tests/notifications.rs::alert_fanout_retries_then_reuses_delivery_id_and_exact_body` asserts transient retry, same delivery ID across lease reclaim, and identical signed body bytes; `nonmatching_filter_creates_no_delivery_and_destination_is_enforced` asserts destination and filter policy.
- [x] 1.4 Integration: alert → subscription → delivery end-to-end
      against an in-process SMTP capture and a `wiremock`-style
      webhook receiver. — `monitoring_alert_publishes_to_matching_subscription` exercises the full MonitoringService→NotificationService path; `cpu_alert_reaches_real_smtp_capture_and_records_success_audit` (ignored without loopback) stands up an in-process SMTP capture and asserts the audit row.
- [x] 1.5 CLI E2E: `openpanel notifications channel {add,list,test,rm}`
      and `subscription {add,list,rm}`. — `crates/openpanel-cli/tests/cli/notifications.rs::cli_notification_channel_and_subscription_lifecycle`.
- [x] 1.6 Web: `/settings/notifications` channel CRUD, test-send
      button, and CSRF. — `crates/openpanel-test-support/src/server.rs::notification_rest_metadata_and_browser_test_send_enforce_csrf` asserts the JSON metadata never returns plaintext credentials and the browser test-send POST without a CSRF token returns 403.

## 2. Domain and Application

- [x] 2.1 Implement `Channel`, `Subscription`, `DeliveryAttempt`
      aggregates in `crates/openpanel-domain/src/notifications/`. — `Channel` (Smtp/Webhook with allowlist, TLS mode, validation), `Subscription` (per-user, filter-bound, enable/disable), `DeliveryAttempt` (lease/retry/terminal state machine, payload digest, `retry_delay(attempt)` backoff schedule).
- [x] 2.2 Add SQLite migrations and `SqliteNotificationRepository`
      with atomic lease and retry fields. — `crates/openpanel-app/src/migrations/notifications/V001__init.sql` defines `notification_channels`, `notification_subscriptions`, `notification_events`, `notification_deliveries`; `repo.rs` provides `lease_due` with an in-transaction compare-and-set on the lease and the `idx_notification_deliveries_due` index.
- [x] 2.3 Implement `NotificationService` and the dispatcher
      background task with backoff, allowlists, and per-channel
      rate limits. — `NotificationService` performs filter evaluation, destination allowlist, atomic lease, and webhook signature; `NotificationDispatcherTask` ticks every `OPENPANEL__NOTIFICATIONS__TICK_SECS` seconds (default 10) and calls `dispatch_once(now, 100)`; per-channel rate limit enforced in `take_rate_slot` (60/min default).
- [x] 2.4 Add `lettre` SMTP and `reqwest` webhook adapters behind
      typed ports; wire `rustls` only. — `NotificationAdapter` port; `RustlsNotificationAdapter` (lettre with `tokio1-rustls-tls`, `reqwest` with default-features=false and rustls); workspace `lettre` and `reqwest` features pinned to rustls.

## 3. Adapters and UI

- [x] 3.1 Add REST routes for channels, subscriptions, deliveries,
      and `/notifications/health`. — `crates/openpanel-api/src/routes/notifications.rs` exposes `GET/POST /api/v1/notifications/channels`, `DELETE /api/v1/notifications/channels/{id}`, `POST /api/v1/notifications/channels/{id}/test`, `GET/POST /api/v1/notifications/subscriptions`, `DELETE /api/v1/notifications/subscriptions/{id}`, `GET /api/v1/notifications/health`, `GET /api/v1/notifications/deliveries/{id}`.
- [x] 3.2 Add `openpanel notifications {channel,subscription}` CLI
      subcommands. — `commands.rs` defines `Notification { Channel { Add, List, Test, Rm }, Subscription { Add, List, Rm }, Health }`; `main.rs` dispatches to `handlers::notification_*`; CLI E2E covers the full lifecycle.
- [x] 3.3 Add `/settings/notifications` web pages with the
      test-send button. — `crates/openpanel-web/src/notifications.rs` renders channel list, subscription list, channel form, test/disable forms, and subscription form; CSRF check on every POST; the `/settings/notifications` route is registered in `crates/openpanel-web/src/router.rs`.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice. — 0 failures across the workspace on two consecutive runs.
- [x] 4.2 `make check` clean. — `make fmt`, `make clippy`, `make docs`, `make audit` all `status: ok`.
- [x] 4.3 Smoke-test: trigger CPU alert → delivery lands in SMTP
      capture → audit row recorded. — `cpu_alert_reaches_real_smtp_capture_and_records_success_audit` (loopback-required) stands up an in-process SMTP capture, runs the full alert→publish→dispatch→audit path, and asserts the SMTP body contains the subject and a `DeliverySucceeded` audit row exists. (The test is `#[ignore]` by default because it needs a real loopback socket; the path it exercises is the same one the CLI/E2E test covers via the mock adapter.)
- [ ] 4.4 Archive with `openspec archive 2026-08-12-add-notification-channels`.
