# Add Terraform provider and SDK — Design

## ApiContract descriptor

```rust
pub struct ApiContract {
    pub source: OpenApiRef,            // points at openapi.rs output
    pub sdks: Vec<Language>,           // Rust | Go | TypeScript
    pub provider_resources: Vec<ResourceKind>, // Site | User | Dns | Backup
}
```

## Generated SDK surface (contract)

```rust
// Per language, from the same OpenAPI paths:
pub struct Client { auth: Token }
impl Client {
    pub fn sites(&self) -> SitesApi;
    pub fn users(&self) -> UsersApi;
    pub fn dns(&self) -> DnsApi;
    pub fn backups(&self) -> BackupsApi;
}
```

## Terraform provider resources (contract)

```
openpanel_site     { domain, plan, account }
openpanel_user     { email, role, account }
openpanel_dns_zone { domain, records[] }
openpanel_backup   { target, schedule, target_id }
```

## Sync CI flow

```
on api change / schedule:
  regenerate SDK (rust/go/ts) from openapi.rs
  regenerate provider from openapi.rs
  run contract tests: every provider resource maps to a real endpoint
  if generated != committed -> fail pipeline (drift)
```

## Endpoints

```
(no new panel endpoints; SDK/provider call the existing REST API)
```

## Tests

```
1.1 Unit: contract parser maps OpenAPI ops to SDK methods/resources.
1.2 Property: every provider resource has a backing endpoint in the
    contract; SDK error types cover auth + 4xx.
1.3 CI: generated SDK/provider compile against openapi.rs; drift fails.
1.4 Integration: provider apply creates a site via the live API.
1.5 E2E: terraform plan/apply/destroy round-trips a site + dns zone.
```
