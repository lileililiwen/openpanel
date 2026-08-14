# Add notification channels

## Why

Monitoring ships alerts that write to the audit log, but there is no
way to actually be told about them. Baota has email/SMS push; cPanel
has contact channels; every modern panel can notify its operator when
something goes wrong. Without delivery, `AlertFired` events are a
diary nobody reads. This change closes the loop with first-party
**notification channels**: email (SMTP, pure-Rust `lettre`) and
generic webhook (HMAC-signed JSON), wired into monitoring and into a
new "operator event" subscription. No SMS, no third-party push — that
keeps the surface tight and the dependency footprint small.

## What Changes

- New `notifications` bounded context with `Channel` aggregates
  (`Smtp`, `Webhook`), per-user subscriptions, and a dispatcher
  background task.
- Channels are Owner-configured once; users subscribe their own
  preferences to those channels (e.g. "alert me when CPU > 90% on
  email-channel `ops@`").
- Webhook deliveries are POSTs with an HMAC-SHA256 signature header
  (`X-OpenPanel-Signature: sha256=<hex>`) and a bounded JSON body.
- SMTP deliveries use TLS by default (`lettre` with `rustls`),
  support STARTTLS fallback, and never log the message body.
- Retry policy with exponential backoff up to N attempts (default 5);
  terminal failures audited and surfaced in `/notifications/health`.
- REST, CLI, and `/settings/notifications` web surface.

## Capabilities

### New Capabilities

- `notifications`: channel registry, subscriptions, dispatcher,
  delivery audit.

### Modified Capabilities

- `monitoring`: alert firing publishes to the notification
  dispatcher; existing audit-log behavior is preserved as a
  fallback.

## Impact

- Domain: `Channel`, `Subscription`, `DeliveryAttempt` aggregates.
- App: `NotificationService`, dispatcher background task, SMTP
  and webhook adapters behind typed ports.
- API/CLI/web: channel CRUD, subscription CRUD, `/notifications/health`,
  `/settings/notifications`.
- Config: per-channel rate limit, retry policy, SMTP credentials
  loaded from `OPENPANEL__NOTIFICATIONS__SMTP__*`.
