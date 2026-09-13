# Proposal: Complete production ACME lifecycle

## Why

The certificate model, challenge server, nginx rendering, and renewal contract
exist, but `RustlsAcmeClient::issue` still returns a skeleton error. This is a
critical production gap because a panel cannot claim automated HTTPS while its
main issuance path is non-functional.

## What Changes

- Land the issuance state machine (bounded polls, exponential backoff,
  redacted error log) and the stable `IssuanceError` /
  `SslError::AcmeRateLimited` / `SslError::AcmeUnreachable` mapping.
- Land the DNS + port-80 preflight as a stable `PreflightOutcome`
  surfaced through the web/API/CLI.
- Land the 24-hour renewal backoff so a transient failure does not
  hammer the Let's Encrypt API and trip the
  `5-duplicate-certificates-per-week` rate limit.
- Land the nginx-reload hook that runs `nginx -t && nginx -s reload`
  after a successful renewal; on test failure the previous
  on-disk cert files are preserved and the renewal is reported
  as failed.
- The live Let's Encrypt network issuance against
  `rustls-acme 0.13` remains the follow-up change (the
  `AcmeState` stream API does not fit the request/response
  `issue` shape; the follow-up must bridge it).

## Capabilities

### Modified Capabilities

- `ssl`

## Non-goals

- DNS-01 wildcard issuance.
- Certificate authority other than Let's Encrypt.
- Returning private keys through any adapter.
- TLS-ALPN-01 challenges (the spec stays HTTP-01 only).

## Dependencies

Depends on `add-release-and-deployment-governance` for production artifact and
runtime verification.
