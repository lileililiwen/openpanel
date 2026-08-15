## ADDED Requirements

### Requirement: Export Usage

`GET /billing/usage/{owner_id}` SHALL return the aggregated usage
(bandwidth, disk, compute) for an owner over a billing period, derived
from `bandwidth-accounting` and local accounting. The response SHALL
NOT mutate site or quota state.

#### Scenario: Export succeeds

- **WHEN** an Admin or the owner's reseller calls
        `GET /billing/usage/{o1}?period=2026-08`
- **THEN** a `UsageMeter` is returned with bandwidth, disk, and compute
        totals for `o1` in that period.

#### Scenario: Unauthorised caller rejected

- **WHEN** a caller lacking admin/reseller authority for `o1` requests
        usage
- **THEN** the request is rejected with `BillingError::Forbidden`.

### Requirement: Chargeback

The system SHALL convert a `UsageMeter` into a `Chargeback` using the
owner's plan rates from `hosting-plans`. Over-quota usage SHALL be
flagged per `resource-quotas`.

#### Scenario: Chargeback priced

- **WHEN** a `UsageMeter` for `o1` is priced against its plan
- **THEN** a `Chargeback` with amount and currency is produced; any
        usage above the owner's quota is marked `over_quota = true`.

### Requirement: Register Integration

`POST /billing/integration` SHALL register an external billing
connector (WHMCS / Blesta / FOSSBilling) with an endpoint and a
webhook secret. The secret SHALL be encrypted at rest.

#### Scenario: Register succeeds

- **WHEN** an Admin posts a connector with a valid `provider` and
        `endpoint`
- **THEN** an `Integration` row exists with an encrypted secret and
        `enabled = true`; audit `BillingIntegrationAdded{id}` recorded.

#### Scenario: Unsupported provider rejected

- **WHEN** `provider` is not one of the supported billing platforms
- **THEN** the request is rejected with `BillingError::UnknownProvider`.

### Requirement: Provisioning Webhook

`POST /billing/webhook` SHALL verify
`HMAC-SHA256(body, integration_secret)` before relaying a provisioning
or deprovisioning event to enabled integrations. An unverified request
SHALL return `401` and SHALL NOT relay.

#### Scenario: Valid event relayed

- **WHEN** a signed provisioning event arrives
- **THEN** the event is relayed to all enabled integrations and
        `202 Accepted` is returned.

#### Scenario: Bad signature refused

- **WHEN** the signature is invalid or absent
- **THEN** the response is `401`, no event is relayed, and audit
        `BillingWebhookRejected{reason}` records the reason only.
