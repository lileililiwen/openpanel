# Add IPv6 and address pools — Tasks

## 1. Testing

- [x] 1.1 Unit: next-free address selection; shared vs dedicated
      uniqueness enforcement; IPv6 enablement flag.
- [x] 1.2 Property: every allocated address lies inside its pool CIDR;
      a dedicated address is never assigned to two sites.
- [x] 1.3 Service: allocate to a site, deallocate, and rebind the
      vhost to the new address set.
- [x] 1.4 Integration: the vhost serves on an assigned IPv4 and an
      assigned IPv6 address.
- [ ] 1.5 CLI E2E: `openpanel ip pools` -> `assign`.
- [ ] 1.6 Web: Network/IP tab (CSRF), pool list, assign control.

## 2. Domain and Application

- [x] 2.1 Implement `IpPool`, `IpAllocation`, `SiteAddress`,
      `IpStatus` under `crates/openpanel-domain/src/ip_allocation/`.
- [x] 2.2 Add SQLite migration for `ip_pools`, `ip_allocations`.
- [x] 2.3 Implement `IpService`, `Allocator`, `VhostBinder`; register
      the module via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/admin/network/ip-pools` and `/sites/{id}/ip` REST
      routes (admin-only for pools, owner-scoped for site IP).
- [ ] 3.2 Add `openpanel ip {pools,assign}` CLI.
- [ ] 3.3 Build the Network/IP tab (CSRF), pool list, assign control.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: create an IPv4 and an IPv6 pool, assign a
      dedicated IPv6 to a site, confirm the vhost binds it; deallocate
      and confirm it is released.
- [x] 4.4 Archive with `openspec archive add-ipv6-and-address-pools`.
