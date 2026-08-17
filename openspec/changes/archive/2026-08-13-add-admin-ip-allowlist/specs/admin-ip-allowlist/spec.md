## Purpose

Adds a network-level allowlist for panel authentication and
write endpoints, evaluated BEFORE session or token
authentication. Together with the existing `host-security`
firewall this gives a layered "remote-only-from-my-LAN"
control with a documented break-glass bypass.

# admin-ip-allowlist Specification

## Requirements

### Requirement: Allowlist Middleware Before Auth

`IpAllowlistMiddleware` SHALL run before any session or token
auth middleware on every request. In `AllowlistOrOpen` mode the
absence of an allowlist leaves the panel open; in
`AllowlistStrict` mode the absence of an allowlist denies
every non-matching peer. The middleware SHALL evaluate the
client peer address against the active allowlist and SHALL
support IPv4 and IPv6 CIDRs.

#### Scenario: Strict mode refuses unknown IP

- **WHEN** `mode=AllowlistStrict`, allowlist has `["10.0.0.0/8"]`, and a request arrives from `203.0.113.7`
- **THEN** the response is 403 with `peer_denied{reason=cidr_unmatched}` and audit `IpAllowlistDenied` is recorded.

#### Scenario: Or-Open mode allows when list is empty

- **WHEN** `mode=AllowlistOrOpen` and the allowlist is empty
- **THEN** the middleware is a no-op and the request flows to session auth.

### Requirement: Per-Role Overrides

The allowlist MAY define `role_overrides: BTreeMap<Role,
AllowlistOverride>`. A `Bypass` override for a role skips the
middleware for principal-bound requests of that role; the
override is logged and audited on every application.

#### Scenario: Owner role bypass

- **WHEN** an Owner-role principal authenticates and `role_overrides[Owner] = Bypass`
- **THEN** the middleware is skipped and audit `IpAllowlistRoleBypass` is recorded.

#### Scenario: Bypass does not apply cross-role

- **WHEN** a User-role principal authenticates in the same allowlist
- **THEN** the middleware applies normally.

### Requirement: Bypass Token (Break-Glass)

The middleware SHALL accept `X-OpenPanel-Allowlist-Bypass:
<token>` header where `<token>` matches
`OPENPANEL__SECURITY__ALLOWLIST_BYPASS_TOKEN` (config). The
bypass IS valid for at most 60 seconds per token presentation
and SHALL be compared in constant time. The bypass emits
`IpAllowlistBypassed{actor_ip, expires_at}` audit. A bypass
used outside a lockout window SHOULD be reviewed via the
audit log.

#### Scenario: Valid token

- **WHEN** a request arrives from `203.0.113.7` with the correct bypass token in the header
- **THEN** the request is admitted; audit `IpAllowlistBypassed` is recorded; the bypass does not persist.

#### Scenario: Wrong token

- **WHEN** the bypass token does not match the configured value
- **THEN** the request is denied; audit `IpAllowlistBypassAttemptFailed` is recorded.

### Requirement: Allowlist CRUD

Authorized callers SHALL GET, PUT, and TEST the allowlist. PUT
requires the caller to be an Owner or Admin; a non-empty PUT
must include at least one entry; and the test endpoint SHALL
return whether a given IP is allowed without making a
mutation.

#### Scenario: PUT with empty entries rejected

- **WHEN** an Owner sends `PUT` with `entries: []` and `mode=AllowlistStrict`
- **THEN** the request is rejected with `empty_entries_forbidden`.

#### Scenario: TEST without mutation

- **WHEN** an Admin sends `POST /admin/security/ip-allowlist/test { ip: "1.2.3.4" }`
- **THEN** the response is `{ allowed: true, matched_cidr: "1.2.0.0/16" }` and no audit side-effect.

### Requirement: Lockout Prevention

The middleware SHALL refuse to apply a PUT that would deny all
currently-reachable admin peers. It SHALL require a typed
`confirmed_at` timestamp within ±60 seconds when the new state
would result in zero matches from the configuration's known
admin peers (read from the audit log).

#### Scenario: Lockout detected

- **WHEN** the new allowlist would exclude every known admin peer
- **THEN** the PUT is rejected with `would_cause_lockout` and the audit `IpAllowlistLockoutPrevented` is recorded; the previous allowlist remains in force.

#### Scenario: Lockout overridden

- **WHEN** the same PUT is followed by a fresh `confirmed_at` and the bypass-token header
- **THEN** the PUT is accepted and audit `IpAllowlistLockoutOverridden` records both.
