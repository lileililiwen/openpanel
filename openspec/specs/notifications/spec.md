# notifications Specification

## Purpose
TBD - created by archiving change 2026-08-12-add-notification-channels. Update Purpose after archive.
## Requirements
### Requirement: Notification Channels

The system SHALL let an Owner register `Smtp` and `Webhook` channels. Channel credentials SHALL be stored encrypted at rest under the panel master key; webhook secrets SHALL be HMAC-SHA256 secrets for outbound signing. Each channel has an allowlist of destinations (recipient addresses for SMTP, host CIDRs for webhook) that the dispatcher MUST enforce before delivery.

#### Scenario: Register SMTP channel

- **WHEN** an Owner posts `{ kind: "smtp", host, port, username, password, from_addr, tls_mode }`
- **THEN** the channel is persisted with the password encrypted and a redacted view returned.

#### Scenario: Register webhook channel

- **WHEN** an Owner posts `{ kind: "webhook", url, signing_secret }`
- **THEN** the channel is persisted with the signing secret encrypted under the panel master key and a redacted view is returned.

#### Scenario: Disallowed destination

- **WHEN** a subscription would send to an address or host outside the channel's allowlist
- **THEN** the delivery is rejected before the adapter is called and audited as `DeliveryRejected`.

### Requirement: User Subscriptions

A user SHALL create subscriptions that match `alert`, `audit`, or `job-terminal` events and bind them to a channel and destination. Filters SHALL narrow the events to those the subscriber cares about. A subscription with no matching events SHALL NOT generate any delivery.

#### Scenario: Subscribe to alerts above threshold

- **WHEN** a user creates a subscription with filter `{ metrics: ["cpu_percent"], severity_at_least: "warning" }`
- **THEN** only matching alerts are delivered; lower-severity and other metrics are filtered out before delivery.

#### Scenario: Unsubscribe

- **WHEN** a user disables a subscription
- **THEN** no further events are delivered but the record is retained for audit.

### Requirement: Dispatcher and Delivery Semantics

A background dispatcher SHALL lease pending deliveries, evaluate filters, call the channel adapter, and record the result. Transient failures SHALL retry with exponential backoff (1m, 5m, 30m, 2h, 12h); permanent failures SHALL be marked terminal and audited. Across process restarts delivery SHALL be at-least-once with the same delivery ID and payload digest so cooperating webhook receivers can deduplicate a reclaimed lease.

#### Scenario: Transient retry

- **WHEN** an SMTP adapter returns a 4xx response
- **THEN** the delivery is rescheduled with the next backoff window and `attempt_n` increments.

#### Scenario: Permanent failure

- **WHEN** a webhook returns `410 Gone` or SMTP returns a permanent error code
- **THEN** the delivery is marked `TerminalFailure`, the subscription's failure counter is incremented, and an audit row is written.

#### Scenario: Crash recovery

- **WHEN** the process is killed mid-delivery
- **THEN** on restart the lease is reclaimed and re-attempted with the same delivery ID and payload digest without creating a second delivery record.

### Requirement: Webhook Signing and Payload

Webhook deliveries SHALL POST a JSON body with headers `X-OpenPanel-Signature: sha256=<hex>`, `X-OpenPanel-Delivery: <id>`, and `X-OpenPanel-Event: <kind>`. The body SHALL be HMAC-SHA256 over the raw bytes with the channel's signing secret. Payload size is bounded; secrets are never included.

#### Scenario: Replay protection

- **WHEN** a receiver tracks `X-OpenPanel-Delivery` IDs and rejects duplicates
- **THEN** the dispatcher does not re-send a delivery ID that is already `TerminalSuccess`.

### Requirement: Notifications Health

`GET /api/v1/notifications/health` SHALL return per-channel counters over the last hour (`delivered`, `failed`, `pending`, `oldest_pending_at`) and the next scheduled retry. Owners use this to detect broken relays before alerts go silent.

#### Scenario: Detect dead channel

- **WHEN** a channel has zero deliveries in the last hour while subscriptions exist
- **THEN** the response marks the channel `degraded` and the next retry is included.

### Requirement: Notification Surfaces

REST, CLI, and `/settings/notifications` web surfaces SHALL support channel CRUD, subscription CRUD, test-send, and health. Browser mutations MUST enforce CSRF. SMTP message bodies SHALL NEVER appear in any log line, audit row, or API response.

#### Scenario: Browser test-send enforces CSRF

- **WHEN** an authenticated Owner submits a channel test-send without a valid CSRF token
- **THEN** the panel returns 403 and does not call the SMTP or webhook adapter.

