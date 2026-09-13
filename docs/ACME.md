# ACME / Let's Encrypt runbook

The `openpanel` panel issues TLS certificates through Let's Encrypt
using the ACME HTTP-01 challenge. This runbook is the operator
guide for staging → production, debugging issuance failures, and
the renewal contract.

## Endpoints

| Environment | URL | Default? |
| --- | --- | --- |
| Staging    | `https://acme-staging-v02.api.letsencrypt.org/directory` | **Yes** — every fresh install ships with staging so accidental production rate-limits are impossible. |
| Production | `https://acme-v02.api.letsencrypt.org/directory` | Opt-in via the `ssl.acme.production` configuration knob. |

Production MUST be set explicitly. There is no command that flips
the default — staging is the safe starting point and the operator
has to consciously change it.

## Issuance state machine

Every `POST /api/v1/ssl/certificates/acme` (or `openpanel ssl issue`)
drives the following sequence, with bounded retries and redacted
errors at every step:

1. **Preflight** — `PreflightOutcome` (`Ok`, `DnsFailure`,
   `PortUnreachable`, `ChallengeRouting`, `Timeout`). DNS resolution
   + TCP-connect to port 80 with a 5 s per-step timeout. If the
   outcome is not `Ok`, the issuance refuses to start and writes
   `last_error` with a `preflight: <kind>` prefix.
2. **Order** — the panel calls Let's Encrypt and registers a
   new order for the domain.
3. **Challenge** — the local challenge server (on
   `127.0.0.1:9080`) is registered with the
   `key_authorization`, then Let's Encrypt is told to validate.
4. **Poll** — the order is polled at 2 s, 4 s, 8 s, 16 s, 32 s,
   60 s (capped). After 6 attempts (`MAX_POLL_ATTEMPTS`) the
   issuance returns `Timeout`.
5. **Finalize** — the panel submits a CSR. LetsEncrypt validates
   and returns the certificate URL.
6. **Poll again** — same backoff schedule. The `Valid` status carries
   the cert URL.
7. **Fetch + persist** — the cert + chain are written to
   `/etc/openpanel/ssl/certs/<domain>.crt` and
   `/etc/openpanel/ssl/keys/<domain>.key` (mode 0600). The DB row
   stores the encrypted private key. `last_attempt_at = now`.
8. **nginx reload** — the renewal task runs `nginx -t && nginx -s
   reload` after a successful renewal. If `nginx -t` fails, the
   previous on-disk cert files are preserved and the renewal is
   reported as failed.

The challenge token is **always** unregistered from the local
server on every exit path (success, failure, timeout). Stale
tokens would otherwise leak the panel's `key_authorization` to
the next issuance.

## Error classification

Raw ACME strings are mapped to a stable `IssuanceError` enum
(`Challenge`, `RateLimited`, `Unreachable`, `Invalid`, `Timeout`,
`Network`, `Internal`) and from there to a stable `SslError`
variant. The mapping is pure (no I/O, no clock) so the offline
`MockAcmeClient` and the production `RustlsAcmeClient` produce
the same classification for the same input.

| Raw ACME hint | `IssuanceError` | `SslError` | HTTP (API) |
| --- | --- | --- | --- |
| `no http-01 challenge` | `Challenge` | `AcmeChallenge` | 502 `ssl_acme_failed` |
| `urn:ietf:params:acme:error:rateLimited` | `RateLimited` | `AcmeRateLimited` | 429 `ssl_acme_rate_limited` |
| DNS NXDOMAIN / connection refused | `Unreachable` | `AcmeUnreachable` | 502 `ssl_acme_unreachable` |
| `order is invalid` / `caa forbids` | `Invalid` | `Acme` | 502 `ssl_acme_failed` |
| timeout after 6 polls | `Timeout` | `Acme` | 502 `ssl_acme_failed` |
| TLS / HTTP / network error | `Network` | `Acme` | 502 `ssl_acme_failed` |
| anything else | `Internal` | `Acme` | 502 `ssl_acme_failed` |

`Authorization: Bearer …`, JWS-shaped nonces, and PEM blocks are
stripped from every error string before it reaches the audit log
or the `last_error` column. The redactor is idempotent.

## Renewal contract

The renewal scheduler (`SslRenewalTask`) wakes every 24 hours and
scans every ACME-issued cert. A cert is re-issued when both:

- `valid_to - now <= 30 days` (the renewal window), AND
- `now - last_attempt_at > 24h` (the backoff window).

A cert that failed today is not retried until tomorrow — this
honours Let's Encrypt's `5-duplicate-certificates-per-week`
rate limit. The next earliest retry is the next daily tick
(≈ `last_attempt_at + 24h`).

On a successful renewal the service runs `nginx -t && nginx -s
reload`. If the test fails, the previous on-disk cert files are
preserved and the renewal records the test failure as `last_error`.
The DB row is not updated with the half-broken files.

## Staging → Production checklist

1. **Staging first.** Always issue a staging cert for a new domain
   and confirm the operator dashboard shows the cert with
   `acme_endpoint = staging`.
2. **Preflight.** `GET /api/v1/ssl/certificates/{domain}/preflight`
   should return `{"kind": "ok"}` before any issuance. If it
   returns `dns` or `port`, fix the DNS / firewall first.
3. **Flip to production.** Set `ssl.acme.production = true` in the
   panel config and restart. There is no other knob.
4. **Re-issue.** Re-run the issuance; the cert now has
   `acme_endpoint = production` and is publicly trusted.
5. **Confirm the audit trail.** `GET /api/v1/audit?module=ssl` shows
   the redacted `SslIssued` event with `endpoint: production` in
   its metadata.
6. **Backoff after rate limits.** If you see
   `SslError::AcmeRateLimited` (HTTP 429), the scheduler will
   back off for 24 h. Use the staging environment to debug.

## Manual recovery

If a cert is stuck (e.g. nginx config invalid, the renewal
`nginx -t` is failing):

1. `openpanel ssl list` — see every cert, its `status`, `valid_to`,
   and `last_error`.
2. `openpanel ssl issue <domain> --production` — force a re-issue.
3. `openpanel ssl revoke <domain>` — mark a cert revoked (CA
   revocation is a separate flow and is out of scope here).
4. `openpanel ssl delete <domain>` — drop the row and the on-disk
   files.

The renewal scheduler never re-issues a `Manual` or `SelfSigned`
cert. It only handles `Acme`.

## Live wiring status

The `RustlsAcmeClient` returns a clear
`SslError::Acme("...gated on the follow-up tls change")` until the
follow-up change bridges the high-level `rustls_acme::AcmeState`
stream API into the request/response `issue` shape. The
`MockAcmeClient` covers the offline end-to-end path so the rest of
the panel (renewal, encryption, persistence, audit) is fully
tested today. See `docs/TODOS.md` entry #1.
