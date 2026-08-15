# Add load balancing and failover

## Why

The `cluster-data-model` bounded context (active) defines node roles,
topology, shared storage, and a replicated database, but specifies **no
load-balancing or failover behaviour**. A multi-node cluster therefore
has no way to spread traffic across healthy site backends or to recover
when a node dies. Production hosts (cPanel/WHM with a load balancer,
Plesk with Keepalived, CloudPanel, 1Panel) treat HA traffic routing as
a base expectation. This change adds a `load-balancing` bounded context
so HTTP/TCP traffic can be routed across site backends on multiple
cluster nodes with health checks and automatic failover.

## What Changes

- New bounded context `load-balancing` carrying the `Pool`, `Member`,
  and `HealthCheck` aggregates plus `LbService`.
- HTTP and TCP load balancing across site backends located on multiple
  cluster nodes; periodic health checks; automatic failover of
  unhealthy members out of rotation; optional sticky sessions.
- New endpoints: `GET /lb/pools`, `PUT /lb/pools`,
  `GET /lb/pools/{id}/members`, `PUT /lb/pools/{id}/members`,
  `GET /lb/health`.
- A health probe removes a member from rotation when failing and
  restores it after consecutive successful checks.

## Capabilities

### New Capabilities

- `load-balancing`: balance HTTP/TCP traffic across site backends on
  multiple cluster nodes with health checks, automatic failover, and
  optional sticky sessions.

## Impact

- Domain: `Pool`, `Member`, `HealthCheck`, `LbStatus`.
- App: `LbService`, `HealthProbe`, `MemberRotator`.
- API/CLI/web: `/lb/pools`, `/lb/pools/{id}/members`, `/lb/health`;
  CLI `openpanel lb {list,set,members}`; web Load Balancer tab.
- Security: membership changes require admin/cluster authority; health
  data is read-only to non-admins; backend addresses are never exposed
  to site owners.
- Coupling: depends on `cluster-data-model` for node roles and topology;
  routes traffic to backends described by `sites`; emits probe/health
  metrics consumed by `monitoring`.
