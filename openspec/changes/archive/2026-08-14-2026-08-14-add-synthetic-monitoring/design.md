# Add Synthetic Monitoring — Design

## SyntheticCheck model

```rust
pub struct SyntheticCheck {
    pub id: CheckId,
    pub site_id: Option<SiteId>,   // None => host-level check
    pub label: String,
    pub kind: CheckType,           // Http | Tcp | SslExpiry
    pub target: String,            // URL or host:port
    pub interval_secs: u32,        // schedule period
    pub timeout_ms: u32,
    pub expected_status: Option<u16>, // HTTP only
    pub warn_before_secs: Option<u64>, // SSL expiry warning window
    pub enabled: bool,
    pub last_run_id: Option<CheckRunId>,
}
```

## Probe flow

```
schedule(check):
  every interval_secs -> enqueue CheckRunner::run(check)

run(check):
  match kind:
    Http      -> GET target, compare status, measure latency
    Tcp       -> connect target:port, measure connect time
    SslExpiry -> TLS handshake, read not_after, compute remaining
  store CheckResult{status, latency_ms, detail_hash, ts}
  if degraded/failed -> emit alert via notification-channels
  update check.last_run_id
```

## Endpoints

```
GET  /api/v1/monitoring/checks
POST /api/v1/monitoring/checks          body { kind, target, interval_secs?, ... }
GET  /api/v1/monitoring/checks/{id}/run
POST /api/v1/monitoring/checks/{id}/run  (force immediate run)
```

## Tests

```
1.1 Unit: HTTP status compare; TCP connect timeout; SSL remaining-days
      math; alert decision (ok/warn/fail).
1.2 Property: malformed target rejected; run idempotent within window.
1.3 Service tests w/ mock transport: create, schedule, alert on fail.
1.4 Integration: live HTTP probe records result; expired cert warns.
1.5 Web: Monitoring tab lists checks + last result + run button.
```
