# Add hosting plans — Design

## Aggregate

```rust
pub struct HostingPlan {
    pub id: PlanId,
    pub name: String,                    // unique, 3..=64
    pub description: String,
    pub prices: Vec<PlanPrice>,           // display only; amount + currency
    pub features: BTreeMap<PlanFeature, PlanFeatureState>,
    pub quota_caps: PlanQuotas,
    pub allowed_apps: Vec<AppId>,
    pub allowed_php_runtimes: Vec<PhpRuntimeRef>,
    pub status: PlanStatus,               // Active | Disabled
    pub created_at, updated_at, audit_meta,
}

pub enum PlanFeatureState { Disabled, Optional, Required }
pub struct PlanQuotas {
    pub disk_bytes: u64,
    pub bandwidth_bytes_per_month: u64,
    pub max_sites: u32,
    pub max_databases: u32,
    pub max_mail_domains: u32,
    pub max_mailboxes: u32,
    pub max_cron_jobs: u32,
    pub max_api_tokens: u32,
    pub max_fleet_agents: u32,
}
```

## Resolver

```rust
trait PlanResolver {
    fn effective(&self, user: &User) -> EffectiveQuotas;   // plan ∩ role
}
```

The resolver is the single read-side consumer for the
`resource-quotas` and other quota-driven changes.

## Endpoints

```
GET    /api/v1/hosting-plans                  active + disabled
POST   /api/v1/hosting-plans                  body: HostingPlanCreate
PUT    /api/v1/hosting-plans/{id}             body: HostingPlanUpdate
DELETE /api/v1/hosting-plans/{id}             refuses if assigned
POST   /api/v1/hosting-plans/{id}/clone       body: {name}
POST   /api/v1/hosting-plans/{id}/assign      body: { user_id }
POST   /api/v1/hosting-plans/{id}/unassign    body: { user_id }
```

Plan deletion requires the assignment table to be empty for that
plan; otherwise the response is `409 Conflict` with
`plan_in_use`.

## CLI

```
openpanel plans list
openpanel plans show <id>
openpanel plans create <name> [--disk …] [--bandwidth …] [--sites N] …
openpanel plans update <id> [--disk …]
openpanel plans assign <plan_id> <user_id>
openpanel plans unassign <plan_id> <user_id>
openpanel plans disable <id>
```

## Tests

```
1.1  Unit: PlanValidator (caps, allowed_apps, allowed_php);
      PlanResolver effective = max(0, min(plan, role)).
1.2  Property: assignments are 1:1 (one current assignment per
      user); effective quota ≤ plan quota; min(...) bound is
      enforced.
1.3  Service tests with mock audit: create/update/disable, clone,
      assign/unassign; refuse deletion while assigned.
1.4  Integration: POST plan, assign to user, PUT quota cap,
      unassign, delete; audit events present.
1.5  CLI E2E: create plan → assign → resolve via mock consumer.
1.6  Web: plan list page, plan editor, assign dialog (CSRF).
```
