# Add reseller billing integration — Tasks

## 1. Testing

- [x] 1.1 Unit: chargeback pricing math; billing-period windowing;
      integration webhook HMAC verify (valid/invalid).
- [x] 1.2 Property: exported usage never exceeds the underlying
      accounting totals; repeated export of a closed period is stable.
- [x] 1.3 Service: export usage per owner, compute chargeback, flag
      over-quota usage per `resource-quotas`.
- [x] 1.4 Integration: a signed provisioning webhook fans out only to
      enabled integrations; bad signature refused.
- [ ] 1.5 CLI E2E: `openpanel billing usage {owner}` -> `integrate`.
- [ ] 1.6 Web: Billing tab (CSRF), usage chart, integration status.

## 2. Domain and Application

- [x] 2.1 Implement `UsageMeter`, `Chargeback`, `Integration`,
      `BillingStatus` under
      `crates/openpanel-domain/src/billing/`.
- [x] 2.2 Add SQLite migration for `usage_meters`, `chargebacks`,
      `billing_integrations`.
- [x] 2.3 Implement `BillingService`, `UsageExporter`,
      `ChargebackEngine`, `WebhookRelay`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/billing/usage/{owner_id}`, `/billing/integration`,
      `/billing/webhook` REST routes (admin/reseller authority).
- [ ] 3.2 Add `openpanel billing {usage,integrate}` CLI.
- [ ] 3.3 Build the Billing tab (CSRF), usage chart, integration
      status.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: export an owner's usage, register a mock
      integration, fire a signed provisioning webhook, confirm it is
      relayed; send a bad-signature webhook, confirm refusal.
- [x] 4.4 Archive with `openspec archive add-reseller-billing-integration`.
