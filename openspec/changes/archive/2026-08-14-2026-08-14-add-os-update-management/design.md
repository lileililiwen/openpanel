# Add OS Update Management — Design

## UpdatePolicy model

```rust
pub struct UpdatePolicy {
    pub unattended_security: bool,  // unattended-upgrades on/off
    pub auto_reboot: bool,          // reboot after kernel update
    pub allow_other_updates: bool,  // non-security auto-apply
    pub last_policy_edit: DateTime<Utc>,
}
```

## Update flow

```
list_updates():
  run `apt-get -s upgrade` + security origin filter
  parse -> Vec<PackageUpdate{kind: Security|Other, name, version, size}

apply_security():
  run `apt-get install --only-upgrade $(security pkgs)` (allow-listed)
  capture history; set RebootState if kernel touched; audit

set_policy(policy):
  write /etc/apt/apt.conf.d/50unattended; audit policy change
```

## Endpoints

```
GET  /api/v1/admin/os/updates          -> { updates[], reboot_required }
POST /api/v1/admin/os/updates/apply    body { scope: "security"|"all" }
PUT  /api/v1/admin/os/updates/policy   body UpdatePolicy
```

## Tests

```
1.1 Unit: apt security-origin parsing; reboot flag derivation.
1.2 Property: apply payload only ever maps to allow-listed apt args.
1.3 Service tests w/ mock apt: list, apply-security, set policy.
1.4 Integration: real apt dry-run lists updates; policy written.
1.5 Web: OS Updates panel lists security vs other + apply button.
```
