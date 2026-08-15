# Add IPv6 and address pools — Design

## IpPool model

```rust
pub struct IpPool {
    pub id: PoolId,
    pub family: IpFamily,           // V4 | V6
    pub cidr: IpNet,                // range owned by the panel
    pub shared: bool,               // shared vs dedicated pool
    pub allocations: Vec<IpAllocation>,
}

pub struct IpAllocation {
    pub address: IpAddr,
    pub site_id: SiteId,
    pub owner_id: OwnerId,          // from account-hierarchy
    pub shared: bool,
    pub enabled: bool,
    pub ipv6: bool,
}

pub struct SiteAddress {
    pub site_id: SiteId,
    pub v4: Option<IpAddr>,
    pub v6: Option<IpAddr>,
}
```

## Allocation flow

```
allocate(pool, site_id, owner_id, want_v6):
  addr = Allocator.next_free(pool)        // reject if none free
  if shared: allow multiple sites on addr else unique
  allocation = IpAllocation{addr, site_id, owner_id, shared, ipv6}
  VhostBinder.bind(site_id, addrs)        // update vhost + reload
  return allocation
```

## Endpoints

```
GET  /api/v1/admin/network/ip-pools        list pools
PUT  /api/v1/admin/network/ip-pools        create/update pool
GET  /api/v1/sites/{id}/ip                site address(es)
PUT  /api/v1/sites/{id}/ip                assign from pool(s)
```

## Tests

```
1.1 Unit: next-free selection; shared vs dedicated uniqueness.
1.2 Property: allocated address always inside pool CIDR; no double
    assign of a dedicated address.
1.3 Service tests w/ mock binder: allocate, deallocate, vhost rebind.
1.4 Integration: vhost serves on assigned IPv4 and IPv6.
1.5 CLI E2E: ip pools -> assign.
1.6 Web: Network/IP tab (CSRF), pool list, assign control.
```
