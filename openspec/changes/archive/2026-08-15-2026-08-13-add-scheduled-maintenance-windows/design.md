# Add scheduled maintenance windows — Design

## Window shape

```rust
pub struct MaintenanceWindow {
    pub id: WindowId,
    pub label: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub affected_action_classes: Vec<DestructiveActionClass>,
    pub allow_reads: bool,                 // default true
    pub allow_overrides: bool,             // default false
    pub notification_recipient: Option<UserId>,
    pub audit_meta,
}

pub enum DestructiveActionClass {
    SoftwareInstall,
    SoftwareUpdate,
    SoftwareUninstall,
    ContainerCreate, ContainerDelete,
    BackupRestore,
    WebAppInstall, WebAppUpgrade, WebAppUninstall,
    SiteClone, SitePromote, SiteDestroy,
    DatabaseDrop,
    MailDomainDelete,
    SnapshotQuarantine,
}
```

## Enforcer

```rust
trait MaintenanceEnforcer {
    fn check(&self, class: DestructiveActionClass, actor: UserId)
        -> Result<(), MaintenanceError>;
}
```

The enforcer is a thin wrapper consulted by all bounded
contexts before a destructive call. During an active window,
the call returns `MaintenanceError::InMaintenanceWindow{class,
window_id}` unless an `allow_overrides=true` and a typed
override token is supplied.

## Override

```
POST /admin/maintenance/override
  body: { window_id, action_class, confirmed_at, reason }
  → 200 { override_token, expires_at = now + 30min }
```

The token is short-lived (≤ 30 minutes), recorded in the
audit, and consumed once. Subsequent calls require a fresh
token.

## Endpoints

```
GET    /api/v1/admin/maintenance
POST   /api/v1/admin/maintenance             body: MaintenanceWindowCreate
PUT    /api/v1/admin/maintenance/{id}
DELETE /api/v1/admin/maintenance/{id}
POST   /api/v1/admin/maintenance/override    body: { window_id, action_class, ... }
```

## CLI

```
openpanel admin maintenance list
openpanel admin maintenance set   --start <ts> --end <ts> \
       --classes software-install,backup-restore \
       [--allow-overrides]
openpanel admin maintenance clear <id>
openpanel admin maintenance override <window_id> --class <c> \
       --reason "<text>" --confirmed-at <ts>
```

## Tests

```
1.1  Unit: action-class discrimination; window-bound check;
      override token TTL.
1.2  Property: enforcer never blocks reads when allow_reads;
      override single-use.
1.3  Service tests with mock scheduler and audit.
1.4  Integration: window active + destructive call → blocked.
1.5  CLI E2E.
1.6  Web: /admin/maintenance editor (CSRF); upcoming-window
      banner.
```
