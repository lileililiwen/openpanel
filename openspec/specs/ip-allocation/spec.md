# ip-allocation Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-ipv6-and-address-pools. Update Purpose after archive.
## Requirements
### Requirement: Manage Address Pools

The system SHALL let an admin create and update IPv4 and IPv6 address
pools, each marked shared or dedicated. A pool SHALL define the CIDR it
owns.

#### Scenario: Create pool

- **WHEN** an Admin puts `PUT /admin/network/ip-pools` with a `family`,
        `cidr`, and `shared` flag
- **THEN** an `IpPool` row exists and its addresses become allocatable;
        audit `IpPoolCreated{pool_id}` is recorded.

#### Scenario: Invalid CIDR rejected

- **WHEN** `cidr` is not a valid IPv4/IPv6 network
- **THEN** the request is rejected with `IpError::InvalidCidr`.

### Requirement: Allocate Address

`PUT /sites/{id}/ip` SHALL allocate an address from a pool to a site,
honouring ownership from `account-hierarchy`. A dedicated address SHALL
NOT be assigned to more than one site; a shared address MAY be.

#### Scenario: Allocate dedicated IPv6

- **WHEN** an Admin assigns a dedicated IPv6 from pool `v6` to site `s1`
- **THEN** an `IpAllocation` binds that address to `s1` and the vhost
        is rebound to include it.

#### Scenario: Double-assign dedicated rejected

- **WHEN** a dedicated address already bound to `s1` is requested for
        `s2`
- **THEN** the request is rejected with `IpError::AddressInUse`.

### Requirement: IPv6 Enablement

The system SHALL allow a site to be enabled for IPv6 independent of its
IPv4 assignment, and SHALL track the resulting `SiteAddress` (v4, v6).

#### Scenario: Enable IPv6

- **WHEN** an Admin enables IPv6 on `s1`
- **THEN** `SiteAddress{v6}` is populated and the vhost listens on the
        assigned IPv6 address.

### Requirement: Deallocate Address

`PUT /sites/{id}/ip` with an empty/removed address entry SHALL
deallocate the address, release the allocation, and rebind the vhost.

#### Scenario: Deallocate

- **WHEN** an Admin removes the IPv6 from `s1`
- **THEN** the `IpAllocation` is deleted, the address returns to the
        pool, and the vhost no longer binds it.

### Requirement: Read Site Address

`GET /sites/{id}/ip` SHALL return the site's assigned addresses. An
owner SHALL see only their own sites' addresses.

#### Scenario: Owner reads own site

- **WHEN** an Owner calls `GET /sites/{s1}/ip` for their own site
- **THEN** the response lists `s1`'s v4 and v6 addresses.

#### Scenario: Owner reads another site rejected

- **WHEN** an Owner calls `GET /sites/{s2}/ip` for a site they do not
        own
- **THEN** the request is rejected with `IpError::Forbidden`.

