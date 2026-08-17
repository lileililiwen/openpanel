# Add admin IP allowlist

## Why

`openspec/specs/host-security/spec.md` covers the host firewall
(nftables) but does not pin a **panel-login IP allowlist**. cPanel's
"IP Blocker" (and the administrative login restrict) is a network-
level gate in front of the panel UI/API independent of the host
firewall. OpenPanel currently has only RBAC. Adding this gate
gives a meaningful "remote-only-from-my-LAN" control before any
auth attempt, including failed passwords and CSRF probing.

## What Changes

- New bounded context `admin-ip-allowlist` with the
  `AdminIpAllowlist` aggregate: a list of CIDR ranges + modes
  (`Owner | Admin | User`).
- New middleware in `openpanel-api` evaluated before any
  authentication, with a "fail-to-deny" mode (allowlist present
  → must match; absent → open) and a "fail-to-allow" mode
  (allowlist present and matches → allow; absent → deny) for
  high-security deployments.
- New endpoints: `GET/PUT /admin/security/ip-allowlist`,
  `POST /admin/security/ip-allowlist/test` (does my IP match?).

## Capabilities

### New Capabilities

- `admin-ip-allowlist`: network-level allowlist for panel
  authentication and write endpoints.

## Impact

- Domain: `AdminIpAllowlist`, `IpCidr`, `AllowlistMode`
  (`AllowlistOrOpen | AllowlistStrict`).
- App: `AdminIpAllowlistService`, `IpAllowlistMiddleware`
  inserted in `openpanel-api`.
- API/CLI/web: `/admin/security/ip-allowlist/*`; CLI
  `openpanel admin allowlist {get,set,test,remove}`.
- Configuration: global enable/disable; per-role override.
