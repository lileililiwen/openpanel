# Refine SSL with wildcard DNS-01

## Why

The archived `add-ssl-management` change explicitly **deferred**
wildcard / DNS-01 support, noting it requires a per-provider DNS API.
The archived `add-dns-zone-management` change already implemented a
DNS-01 TXT lease and a provider port that can publish and revoke
`_acme-challenge` records. Today a site with many subdomains still needs
one certificate per name, and a bare wildcard `*.domain` is impossible.
This change wires the two halves together so OpenPanel can issue and
renew a wildcard certificate covering the apex and all first-level
subdomains via DNS-01, reusing the existing DNS provider port.

## What Changes

- Added to the `ssl` capability: issue a wildcard certificate for
  `*.domain` (covering the apex `domain` and the wildcard) via DNS-01,
  using the existing DNS provider port from the `dns` capability to
  publish and clean up the `_acme-challenge` TXT record.
- Automatic renewal reuses the same DNS-01 path; the TXT lease is
  created, the order is finalized, and the lease is always revoked
  afterwards (success or failure).
- Security per Agents.md §6: the ACME challenge responder listens on
  `127.0.0.1:9080` (local-only), and the ACME directory defaults to
  staging until explicitly promoted to production.

## Capabilities

### Modified Capabilities

- `ssl`: extend with DNS-01 wildcard issuance and renewal for
  `*.domain` (apex + wildcard) by reusing the `dns` capability's
  provider port for the `_acme-challenge` TXT lease.

## Impact

- Domain: `CertRequest`, `ChallengeKind` (adds `Dns01`), `DnsLease`,
  `AcmeEndpointMode` (staging | production).
- App: `WildcardIssuer`, `Dns01ChallengeSolver` (uses the `dns`
  provider port), `CertRenewalScheduler`.
- API/CLI/web: `PUT /sites/{id}/ssl` gains a `wildcard: bool` option;
  CLI `openpanel site ssl issue --wildcard`; web SSL tab checkbox
  (CSRF).
- Security: ACME challenge server bound to `127.0.0.1:9080`; staging
  ACME directory by default; TXT lease revoked after every attempt.
- Coupling: depends on the `ssl` capability for cert storage/renewal;
  reuses the `dns` capability's provider port; pairs with
  `refine-dns-with-zone-templates` for zone/provider ergonomics.
