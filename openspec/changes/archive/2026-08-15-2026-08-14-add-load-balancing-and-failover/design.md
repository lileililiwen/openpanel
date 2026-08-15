# Add load balancing and failover — Design

## Pool model

```rust
pub struct Pool {
    pub id: PoolId,
    pub name: String,
    pub protocol: LbProtocol,       // Http | Tcp
    pub listen_port: u16,
    pub sticky: bool,               // sticky sessions
    pub members: Vec<Member>,
    pub health_check: HealthCheck,
}

pub struct Member {
    pub node_id: NodeId,            // from cluster-data-model
    pub site_id: SiteId,            // backend site
    pub address: SocketAddr,        // backend bind addr
    pub weight: u16,
    pub enabled: bool,
}

pub struct HealthCheck {
    pub kind: ProbeKind,            // Http | Tcp
    pub interval_secs: u32,
    pub timeout_secs: u32,
    pub healthy_threshold: u32,     // consecutive success to re-enable
    pub unhealthy_threshold: u32,   // consecutive fail to disable
}
```

## Failover / rotation flow

```
rotate(pool):
  for member in pool.members:
    ok = HealthProbe.run(member, pool.health_check)
    if !ok: member.failed += 1 else member.failed = 0
    if member.failed >= unhealthy_threshold: member.enabled = false
    if !member.enabled && member.success >= healthy_threshold: member.enabled = true
  write_reload(pool)  // regenerate balancer config + reload
```

## Sticky sessions

When `pool.sticky`, the balancer pins a client (cookie / source IP hash)
to a single enabled member; failover only moves the pin when that member
is disabled.

## Endpoints

```
GET  /api/v1/lb/pools                  list pools
PUT  /api/v1/lb/pools                  create/update pool
GET  /api/v1/lb/pools/{id}/members     list members
PUT  /api/v1/lb/pools/{id}/members     set members + weights
GET  /api/v1/lb/health                 aggregate health summary
```

## Tests

```
1.1 Unit: probe decision (enable/disable thresholds); sticky pin move.
1.2 Property: disabled members never receive traffic; weights sum sane.
1.3 Service tests w/ mock probe: add member, fail, auto-remove, recover.
1.4 Integration: live balancer routes only to enabled members; failover.
1.5 CLI E2E: lb list -> set -> members -> health.
1.6 Web: Load Balancer tab (CSRF), member toggle, health badge.
```
