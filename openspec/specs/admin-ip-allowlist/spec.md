# admin-ip-allowlist Specification

## Purpose

TBD - created by archiving change add-admin-ip-allowlist.

## Requirements

### Requirement: IP Allowlist

The admin-ip-allowlist bounded context SHALL model an `AdminIpAllowlist` carrying `mode: AllowlistMode` (`AllowlistOrOpen | AllowlistStrict`), `entries: Vec<IpCidr>`, `role_overrides: BTreeMap<Role, AllowlistOverride>`, and `updated_at`. The `allows(ip, role)` helper applies the role override (if any) and then the mode: empty allowlist + `AllowlistOrOpen` returns `true`; empty allowlist + `AllowlistStrict` returns `false`.

#### Scenario: Empty allowlist, open mode

- **WHEN** `allows("203.0.113.1", None)` is called with `AllowlistOrOpen` and no entries
- **THEN** the function returns `true`.

#### Scenario: Empty allowlist, strict mode

- **WHEN** `allows("203.0.113.1", None)` is called with `AllowlistStrict` and no entries
- **THEN** the function returns `false`.

### Requirement: CIDR Match

The bounded context SHALL model an `IpCidr` with `address: IpAddr`, `prefix_len: u8`, and `label: Option<String>`. The constructor rejects any prefix length that exceeds the address family (32 for IPv4, 128 for IPv6). The `matches(ip)` helper returns `true` only when `ip` falls within the CIDR.

#### Scenario: Invalid prefix length

- **WHEN** `IpCidr::new("10.0.0.0", 33, None)` is called
- **THEN** the constructor returns `AdminIpAllowlistError::InvalidPrefixLen`.

#### Scenario: IPv4 match

- **WHEN** the CIDR is `10.0.0.0/8` and the IP is `10.0.0.5`
- **THEN** `matches` returns `true`.

#### Scenario: IPv6 match

- **WHEN** the CIDR is `2001:db8::/32` and the IP is `2001:db9::1`
- **THEN** `matches` returns `false`.

### Requirement: Role Override

The bounded context SHALL model `AllowlistOverride` (`Inherit | Bypass`). When `Bypass` is set for a role, the middleware skips the allowlist entirely for that role.

#### Scenario: Bypass role

- **WHEN** `role_overrides[Owner] = Bypass` and the allowlist is `AllowlistStrict` with no entries
- **THEN** `allows(ip, Some(Owner))` returns `true` while `allows(ip, Some(User))` returns `false`.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `IpAllowlistDenied`, `IpAllowlistBypassed`, `IpAllowlistRoleBypass`, and `IpAllowlistLockoutPrevented` audit events.

#### Scenario: Denied address is audited

- **WHEN** a request arrives from a non-allowlisted address
- **THEN** an audit `IpAllowlistDenied` event records the address
        without session content. The bounded context as archived today owns the typed model and the CIDR matcher.
