# Add admin IP allowlist — Design

## Allowlist shape

```rust
pub struct AdminIpAllowlist {
    pub mode: AllowlistMode,
    pub entries: Vec<IpCidr>,
    pub role_overrides: BTreeMap<Role, AllowlistOverride>,
    pub updated_at, audit_meta,
}

pub enum AllowlistMode { AllowlistOrOpen, AllowlistStrict }
pub enum AllowlistOverride { Inherit, Bypass }   // Inherit applies
                                              // the mode;
                                              // Bypass skips
                                              // the middleware
                                              // for this role.
```

## Middleware order

```
Client → ip_allowlist_middleware → rate_limit → session|token →
   csrf (mutations) → route
```

`ip_allowlist_middleware` is consulted BEFORE session/token
authentication so failed-login probes are still rate-limited
and the actor is recorded by peer IP.

## Endpoints

```
GET   /api/v1/admin/security/ip-allowlist
PUT   /api/v1/admin/security/ip-allowlist
        body: { mode, entries: [{cidr, label}], role_overrides? }
POST  /api/v1/admin/security/ip-allowlist/test
        body: { ip: "1.2.3.4" }
        → 200 { allowed: bool, matched_cidr?: "1.2.0.0/16" }
```

## Bypass for break-glass

The allowlist MUST admit a documented **break-glass token**
configured at panel install (`OPENPANEL_ALLOWLIST_BYPASS_TOKEN`).
On lockout, an operator presents the token in a typed header
`X-OpenPanel-Allowlist-Bypass: <token>`; the middleware accepts
the request for 60 seconds and audits `IpAllowlistBypassed`.
Tokens are compared in constant time.

## CLI

```
openpanel admin allowlist get
openpanel admin allowlist set --mode strict --add 10.0.0.0/8 \
      --add 2001:db8::/32
openpanel admin allowlist remove 10.0.0.0/8
openpanel admin allowlist test 1.2.3.4
```

## Tests

```
1.1  Unit: CIDR parsing and IPv4/IPv6 matching; mode behaviour;
      bypass-token constant-time check.
1.2  Property: in AllowlistStrict, an unauthenticated request
      from a non-matching IP is rejected; in AllowlistOrOpen,
      it is accepted only when no allowlist exists.
1.3  Service tests: PUT/GET/test; reverse-order CIDR handling;
      bypass expired token.
1.4  Integration: live middleware blocks a non-matching IP;
      allowlist is consulted before session auth.
1.5  CLI E2E: set, test, remove.
1.6  Web: admin security tab with allowlist editor (CSRF).
```
