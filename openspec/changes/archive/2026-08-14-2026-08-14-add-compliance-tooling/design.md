# Add Compliance Tooling — Design

## AuditRetentionPolicy model

```rust
pub struct AuditRetentionPolicy {
    pub ttl_days: u32,            // records older than this are purged
    pub export_enabled: bool,     // scheduled export of audit log
    pub export_target: Option<String>, // offsite/export sink id
    pub last_edited: DateTime<Utc>,
}
```

## Hardening flow

```
harden(scope):
  for each CIS rule in selected profile:
    capture current state (pre-image)
    apply change (allow-listed, reversible)
    record HardeningRun{rule, pre_image, post_image, status}
  produce report{applied[], failed[], skipped[]}
  every change audited; rollbacks use stored pre_image
```

## GDPR export flow

```
export(user_id):
  gather PII from identity, sites, mail, databases
  redact secrets: db passwords, api tokens, private keys -> "[redacted]"
  produce structured export (JSON/CSV) of remaining PII
  audit GdprExportRequested{user_id, by}
```

## Endpoints

```
POST /api/v1/admin/compliance/harden              body { profile, scope }
GET  /api/v1/admin/compliance/audit-retention
PUT  /api/v1/admin/compliance/audit-retention     body AuditRetentionPolicy
GET  /api/v1/admin/compliance/gdpr-export/{user_id}
```

## Tests

```
1.1 Unit: CIS rule apply/revert with pre-image; TTL purge math;
      secret redaction on export.
1.2 Property: redacted export never contains a raw secret pattern.
1.3 Service tests w/ mock host: harden+report; set retention; export.
1.4 Integration: harden applies and rolls back; export omits secrets.
1.5 Web: Compliance panel (harden wizard, retention form, export).
```
