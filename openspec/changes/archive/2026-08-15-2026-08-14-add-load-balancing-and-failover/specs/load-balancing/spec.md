## ADDED Requirements

### Requirement: Define Pool

The system SHALL let an admin create an HTTP or TCP load-balancing pool
that listens on a port and references site backends located on cluster
nodes. A pool SHALL declare its protocol, whether sticky sessions are
enabled, and a health-check policy.

#### Scenario: Create pool

- **WHEN** an Admin puts `PUT /lb/pools` with a name, `protocol`,
        `listen_port`, and `health_check`
- **THEN** a `Pool` row exists and the balancer config is regenerated
        and reloaded; audit `LbPoolCreated{pool_id}` is recorded.

#### Scenario: Invalid port rejected

- **WHEN** `listen_port` collides with a reserved port
- **THEN** the request is rejected with `LbError::PortInUse`.

### Requirement: Manage Members

`PUT /lb/pools/{id}/members` SHALL set the member list (node, site,
address, weight) for a pool. The system SHALL validate that each member
resolves to a real site backend on a known cluster node.

#### Scenario: Set members

- **WHEN** an Admin sets two members on pool `p1`
- **THEN** `p1` now has two enabled members and traffic is distributed
        across them by weight.

#### Scenario: Unknown site rejected

- **WHEN** a member references a `site_id` that does not exist
- **THEN** the request is rejected with `LbError::UnknownSite`.

### Requirement: Health Check and Failover

The system SHALL periodically probe each member using the pool's health
check. A member that fails `unhealthy_threshold` consecutive probes
SHALL be removed from rotation; a disabled member that passes
`healthy_threshold` consecutive probes SHALL be re-enabled.

#### Scenario: Member fails and is removed

- **WHEN** a backend stops responding for `unhealthy_threshold` probes
- **THEN** the member is set `enabled = false` and stops receiving
        traffic; audit `LbMemberDisabled{member}` is recorded.

#### Scenario: Member recovers

- **WHEN** a disabled member passes `healthy_threshold` probes
- **THEN** the member is re-enabled and again receives traffic.

### Requirement: Sticky Sessions

When a pool has `sticky = true`, the system SHALL pin a client to a
single enabled member. The pin SHALL move only when the pinned member
becomes disabled.

#### Scenario: Sticky pin moves on failover

- **WHEN** a pinned member is disabled by a health check
- **THEN** the client's subsequent requests are routed to another
        enabled member.

### Requirement: Health Summary

`GET /lb/health` SHALL return an aggregate view of pool health
(member counts per status) without exposing backend addresses to
non-admin callers.

#### Scenario: Health summary

- **WHEN** an Admin calls `GET /lb/health`
- **THEN** the response lists per-pool enabled/disabled counts and
        last-probe timestamps.
