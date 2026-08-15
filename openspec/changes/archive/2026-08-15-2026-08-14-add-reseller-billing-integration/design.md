# Add reseller billing integration — Design

## UsageMeter model

```rust
pub struct UsageMeter {
    pub owner_id: OwnerId,           // from account-hierarchy
    pub period: BillingPeriod,       // month window
    pub bandwidth_bytes: u64,        // from bandwidth-accounting
    pub disk_bytes: u64,
    pub compute_units: u64,          // cpu*time
    pub plan_id: PlanId,             // from hosting-plans
}

pub struct Chargeback {
    pub owner_id: OwnerId,
    pub usage: UsageMeter,
    pub amount: Decimal,
    pub currency: String,
}

pub struct Integration {
    pub id: IntegrationId,
    pub provider: BillingProvider,   // Whmcs | Blesta | Fossbilling
    pub endpoint: Url,
    pub secret: WebhookSecret,       // encrypted
    pub enabled: bool,
}
```

## Metering / chargeback flow

```
export_usage(owner_id, period):
  u = UsageExporter.collect(owner_id, period)
      -> bandwidth from bandwidth-accounting
      -> disk/compute from local accounting
  c = ChargebackEngine.price(u, plan_rates(owner.plan_id))
  return UsageMeter + Chargeback
```

## Integration / webhook flow

```
POST /billing/integration  -> register Integration (encrypted secret)
POST /billing/webhook      -> verify HMAC; emit provision/deprovision
                              event to enabled Integration endpoints
```

## Endpoints

```
GET  /api/v1/billing/usage/{owner_id}        meter for owner+period
POST /api/v1/billing/integration             register connector
POST /api/v1/billing/webhook                 signed provision events
```

## Tests

```
1.1 Unit: chargeback pricing; period windowing; HMAC verify.
1.2 Property: usage never exceeds underlying accounting totals.
1.3 Service tests w/ mock accounting: export, chargeback, over-quota.
1.4 Integration: webhook fans out to enabled integrations only.
1.5 CLI E2E: billing usage -> integrate.
1.6 Web: Billing tab (CSRF), usage chart, integration status.
```
