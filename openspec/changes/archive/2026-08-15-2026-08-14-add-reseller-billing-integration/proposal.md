# Add reseller billing integration

## Why

The `account-hierarchy` and `hosting-plans` bounded contexts (both
active) model resellers and the plans assigned to them, but contain
**no billing, chargeback, or metering** behaviour. cPanel's entire
ecosystem is billing-centric — WHMCS, Blesta, and FOSSBilling provision
and bill resellers and their customers through provisioning events.
Without a billing surface, OpenPanel cannot drive chargeback or
integrate with those billing platforms. This change adds a `billing`
bounded context that exports per-owner usage and exposes integration
points for provisioning/deprovisioning events.

## What Changes

- New bounded context `billing` carrying the `UsageMeter`,
  `Chargeback`, and `Integration` aggregates plus `BillingService`.
- Usage metering export: bandwidth (from `bandwidth-accounting`),
  disk, and compute, aggregated per owner. Chargeback hooks translate
  usage into owner-level cost entries.
- Billing integration points for WHMCS / Blesta / FOSSBilling:
  provisioning and deprovisioning event webhooks, plus a registration
  endpoint for an external billing connector.
- New endpoints: `GET /billing/usage/{owner_id}`,
  `POST /billing/integration`, `POST /billing/webhook`.
- Metering is read-only over real accounting data; it never mutates
  site or quota state.

## Capabilities

### New Capabilities

- `billing`: export per-owner usage metering (bandwidth, disk,
  compute), apply chargeback, and expose webhook/integration points for
  WHMCS / Blesta / FOSSBilling provisioning events.

## Impact

- Domain: `UsageMeter`, `Chargeback`, `Integration`, `BillingStatus`.
- App: `BillingService`, `UsageExporter`, `ChargebackEngine`,
  `WebhookRelay`.
- API/CLI/web: `/billing/usage/{owner_id}`, `/billing/integration`,
  `/billing/webhook`; CLI `openpanel billing {usage,integrate}`;
  web Billing tab.
- Security: usage endpoints require admin/reseller authority scoped to
  the owner; integration webhooks are signed; external connectors are
  stored as encrypted secrets.
- Coupling: depends on `account-hierarchy` and `hosting-plans` for
  owner/plan context; reads `bandwidth-accounting` for bandwidth;
  respects `resource-quotas` when reporting over-quota usage.
