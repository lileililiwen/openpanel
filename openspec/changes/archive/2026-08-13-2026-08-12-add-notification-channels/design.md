# Add notification channels — Design

## Domain model

```
Channel (Owner-owned)
  ├─ Smtp    { id, host, port, username, password_enc, from_addr,
  │            tls_mode (Tls | StartTls | None), allowlist[] }
  └─ Webhook { id, url, secret_enc, signing_alg = sha256,
                allowlist[] }

Subscription (per user, per kind)
  { id, user_id, channel_id, destination, kind (Alert | Audit | JobTerminal),
    filters (JSON), enabled }

DeliveryAttempt
  { id, channel_id, subscription_id, payload_digest, status
    (Pending|Retry|TerminalSuccess|TerminalFailure), attempt_n,
    next_retry_at, last_error_redacted }
```

Webhook signing secrets and SMTP passwords use the existing AES-256-GCM
master-key envelope. Plaintext is accepted only at channel creation or
credential rotation and is never returned by metadata endpoints.

## Dispatcher

```
Background task "notification-dispatcher"
  every OPENPANEL__NOTIFICATIONS__TICK_SECS (default 10):
    lease up to BATCH attempts where status=Pending and
        (next_retry_at <= now or attempt_n == 0)
    for each:
        resolve channel via port
        sign / format payload
        call adapter (smtp / webhook)
        on success: status = TerminalSuccess
        on transient failure: attempt_n++, next_retry_at = now + backoff(attempt_n)
        on permanent failure: status = TerminalFailure + audit
```

Backoff: 1m, 5m, 30m, 2h, 12h (5 attempts, then terminal).

Delivery is at-least-once across a crash: a reclaimed lease reuses the
same delivery ID and payload digest so webhook receivers can deduplicate.
SMTP receivers may observe a duplicate when a process dies after the relay
accepts a message but before the success state is committed.

## Webhook payload (bounded, signed)

```
POST <channel.url>
Content-Type: application/json
X-OpenPanel-Signature: sha256=<hex(HMAC-SHA256(secret, body))>
X-OpenPanel-Delivery: <delivery_id>
X-OpenPanel-Event: alert.fired | job.terminal | audit.emitted

{
  "event": "alert.fired",
  "ts": "2026-08-12T10:00:00Z",
  "subject": "cpu_percent > 90 on host01",
  "severity": "warning",
  "details": { ... }            // sanitised, no secrets
}
```

## SMTP

- `lettre` with `rustls` transport.
- Default port 587 + STARTTLS; SMTPS 465 supported.
- Credentials encrypted at rest with the panel master key.
- Subject and headers redacted in logs; body never logged.

## Filters

A subscription's `filters` field is a small JSON schema per kind:

```json
// alert
{ "metrics": ["cpu_percent","memory_percent"],
  "threshold_direction": "above", "severity_at_least": "warning" }

// audit
{ "actions": ["LoginFailed","TwoFactorFailed","TokenCreated"] }

// job
{ "kinds": ["software.install","backup.create"], "outcome": "any|success|failure" }
```

The dispatcher evaluates the filter before sending; the panel never
emits a delivery for an event that did not match any subscription.

## Health

`GET /api/v1/notifications/health` returns per-channel rolling
counters (`delivered_1h`, `failed_1h`, `next_retry_at`,
`oldest_pending`). Owners use this to detect a broken SMTP relay
or a misconfigured webhook before alerts go silent.

## Tests

```
1.1  Unit: filter parsing/evaluation, signature generation,
     backoff schedule, allowlist matching.
1.2  Property: HMAC signature deterministic; backoff strictly
     monotonic; no event matches an empty subscription.
1.3  Service tests with mock SMTP/HTTP adapters, mock clock,
     mock audit; cover transient retry, terminal failure,
     and idempotent re-delivery after process restart.
1.4  Integration: monitoring alert → subscription match → delivery
     record; subscription filter blocks non-matching events.
1.5  CLI E2E: `openpanel notifications channel add/list/test`.
1.6  Web: /settings/notifications UI with channel CRUD, test-send
     button, and CSRF.
```
