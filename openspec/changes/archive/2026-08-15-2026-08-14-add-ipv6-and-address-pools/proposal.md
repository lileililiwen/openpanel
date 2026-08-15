# Add IPv6 and address pools

## Why

No spec in OpenPanel models IPv6 or per-site IP allocation. Sites today
share the host address with no notion of dedicated or shared IPv4/IPv6
assignment. Modern hosts (cPanel/WHM with IPv6 ranges, Plesk, CloudPanel,
1Panel) assign dedicated or shared IPv4 and IPv6 addresses to sites and
expose IP pool management. This change adds an `ip-allocation` bounded
context that enables IPv6 per site and manages address pools so that
vhosts bind to assigned IPs.

## What Changes

- New bounded context `ip-allocation` carrying the `IpPool`,
  `IpAllocation`, and `SiteAddress` aggregates plus `IpService`.
- IPv6 enablement per site; IP address pool management that allocates
  and deallocates dedicated or shared IPv4 and IPv6 addresses to
  sites/owners.
- Bind each site's vhost to its assigned IPs (IPv4 and/or IPv6).
- New endpoints: `GET /admin/network/ip-pools`,
  `PUT /admin/network/ip-pools`, `GET /sites/{id}/ip`,
  `PUT /sites/{id}/ip`.
- Allocation is tracked; an address cannot be double-assigned unless
  explicitly shared.

## Capabilities

### New Capabilities

- `ip-allocation`: enable IPv6 per site, manage IPv4/IPv6 address pools,
  allocate/deallocate dedicated or shared addresses to sites/owners, and
  bind vhosts to assigned IPs.

## Impact

- Domain: `IpPool`, `IpAllocation`, `SiteAddress`, `IpStatus`.
- App: `IpService`, `Allocator`, `VhostBinder`.
- API/CLI/web: `/admin/network/ip-pools`, `/sites/{id}/ip`; CLI
  `openpanel ip {pools,assign}`; web Network/IP tab.
- Security: pool admin is admin-only; an owner may read/assign only
  their own site addresses; shared addresses are explicitly flagged.
- Coupling: depends on `sites` for vhost/docroot binding; respects
  `account-hierarchy` ownership when allocating to an owner.
