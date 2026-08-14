# Add Synthetic Monitoring

## Why

`monitoring` (archived) and `refine-monitoring-with-bandwidth-accounting`
(active) cover host metrics, bandwidth accounting, and host uptime, but
there is **no external HTTP probe or SSL-expiry alerting**. Operators
cannot tell whether a site is reachable from the outside, whether a TCP
port is accepting connections, or whether a certificate is about to
expire until a customer complains. This change adds a
`synthetic-monitoring` bounded context so checks run from outside the
host and failures are surfaced through the existing notification path.

## What Changes

- New bounded context `synthetic-monitoring` carrying the `SyntheticCheck`
  aggregate and `ProbeScheduler`, `CheckRunner`.
- New endpoints: `GET /monitoring/checks`, `POST /monitoring/checks`,
  `GET /monitoring/checks/{id}/run`, `POST /monitoring/checks/{id}/run`.
- Create external synthetic checks per site/URL: HTTP(S) GET, TCP
  connect, and SSL certificate-expiry probes.
- Schedule probes (interval-based) and run them on demand; alert on
  failure or degradation via `notification-channels`.

## Capabilities

### New Capabilities

- `synthetic-monitoring`: define and run external HTTP(S)/TCP/SSL
  synthetic checks per site/URL, schedule probes, and alert on
  failure or degradation.

## Impact

- Domain: `SyntheticCheck`, `CheckResult`, `CheckType`, `CheckStatus`.
- App: `ProbeScheduler`, `CheckRunner`, `SslExpiryInspector`.
- API/CLI/web: `/monitoring/checks*`, on-demand run endpoint; web
  Monitoring tab listing checks + last result.
- Security: probe targets are operator-defined; no user-supplied input
  is executed; results stored without response bodies beyond status.
- Coupling: depends on `monitoring` for metric display; routes alerts
  through `notification-channels`; complements the host-uptime data in
  `refine-monitoring-with-bandwidth-accounting`.
