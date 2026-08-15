# Refine SSL with wildcard DNS-01 — Tasks

## 1. Testing

- [x] 1.1 Unit: `Dns01ChallengeSolver` revokes the TXT lease on both
      success and failure paths; `acme_mode` defaults to staging.
- [x] 1.2 Property: ACME challenge server binds only to
      `127.0.0.1:9080`; a wildcard request produces cert SANs for the
      apex and `*.apex`.
- [x] 1.3 Service: issue (mock DNS provider + mock ACME), renew,
      lease lifecycle; audit `WildcardCertIssued{domains}` records names
      only.
- [x] 1.4 Integration: live staging wildcard issuance; after revoke the
      `_acme-challenge` TXT is gone; apex + wildcard both validate.
- [ ] 1.5 CLI E2E: `openpanel site ssl issue --wildcard` against the
      staging directory.
- [ ] 1.6 Web: SSL tab wildcard checkbox (CSRF), renewal schedule view.

## 2. Domain and Application

- [x] 2.1 Extend `CertRequest` with `ChallengeKind::Dns01`,
      `AcmeEndpointMode`, and `dns_provider`; add `DnsLease` under
      `crates/openpanel-domain/src/wildcard_ssl/`.
- [x] 2.2 Add SQLite migration for `dns_leases` (if not present).
- [x] 2.3 Implement `WildcardIssuer`, `Dns01ChallengeSolver` (using the
      `dns` provider port), and `CertRenewalScheduler`; register via
      `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `wildcard: bool` to `PUT /sites/{id}/ssl` and a
      `GET /sites/{id}/ssl/renewals` route.
- [ ] 3.2 Add `openpanel site ssl issue --wildcard [--provider --mode]`.
- [ ] 3.3 Build the SSL tab wildcard checkbox (CSRF) and renewal
      schedule view.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: issue a staging wildcard cert, confirm apex +
      `*.apex` validate, confirm the TXT record is revoked afterwards.
- [x] 4.4 Archive with `openspec archive refine-ssl-with-wildcard-dns01`.
