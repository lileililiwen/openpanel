# Refine SSL with wildcard DNS-01 — Design

## CertRequest model (extended)

```rust
pub enum ChallengeKind { Http01, Dns01 }

pub struct CertRequest {
    pub site_id: SiteId,
    pub domains: Vec<String>,     // ["domain", "*.domain"]
    pub challenge: ChallengeKind, // Dns01 for wildcard
    pub acme_mode: AcmeEndpointMode, // Staging | Production
    pub dns_provider: ProviderId, // from dns capability
}

pub struct DnsLease {
    pub zone: String,             // _acme-challenge.domain
    pub txt: String,              // ACME challenge value
    pub provider: ProviderId,
}
```

## Wildcard issuance flow

```
issue_wildcard(site_id):
  create CertRequest{ domains: [apex, "*.apex"], challenge: Dns01,
                      acme_mode: Staging (default) }
  lease = dns_provider.upsert_txt(zone="_acme-challenge.<apex>", txt)
  poll ACME until challenge valid (challenge server local at 127.0.0.1:9080)
  finalize order -> download cert bundle
  dns_provider.revoke_txt(lease)        // ALWAYS, success or failure
  store cert; schedule renewal
```

## Renewal

```
renew(site_id):
  reuse issue_wildcard with the same Dns01 + provider
  lease + revoke around the finalize step
```

## Endpoints

```
PUT  /api/v1/sites/{id}/ssl   body { wildcard?: bool, provider?, mode? }
GET  /api/v1/sites/{id}/ssl/renewals
```

## Tests

```
1.1 Unit: TXT lease revoke called on both success and failure paths;
        acme_mode defaults to staging.
1.2 Property: challenge server binds only to 127.0.0.1:9080; cert
        SANs contain apex + *.apex when wildcard requested.
1.3 Service w/ mock dns provider + mock ACME: issue, renew, lease
        lifecycle, revoke-always.
1.4 Integration: live (staging) wildcard issuance; revoke proves TXT
        removed; apex + wildcard both validate.
1.5 CLI E2E: `openpanel site ssl issue --wildcard` on staging.
1.6 Web: SSL tab wildcard checkbox (CSRF), renewal schedule view.
```
