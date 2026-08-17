# Refine backups with resource-scoped restore and remote-target policy — Design

## Resource kinds

```rust
pub enum ResourceKind { Site, Database, MailDomain, Mailbox, AuditLog, Configuration }
```

Restore scope:

```rust
pub enum RestoreScope {
    Site { site_id: SiteId, at: DateTime<Utc> },
    Database { db_id: DatabaseId, at: DateTime<Utc> },
    MailDomain { domain: String, at: DateTime<Utc> },
    Mailbox { email: String, at: DateTime<Utc> },
    AuditLog { from: DateTime<Utc>, to: DateTime<Utc> },
    Configuration { component: String, at: DateTime<Utc> },
}
```

Each variant selects one resource and the exact timestamp from
which to restore. The selection is read-only; the run that
performs the restore is scheduled in `add-offsite-backup-targets`.

## Target kinds

```rust
pub enum BackupTargetKind {
    Local,                 // file:// under config datadir
    OffsiteS3   { bucket: String, region: String, kms_key_id: Option<String> },
    OffsiteRsync{ host: String, user: String, ssh_key_id: String },
    OffsiteB2   { bucket: String, key_id: String },
    OffsiteWasabi{ bucket: String, region: String },
}
```

This change introduces the enum and persists `target_kind` on
plans; storage adapters per kind land in the follow-on change.

## Migration

```sql
ALTER TABLE backup_plans ADD COLUMN target_kind TEXT NOT NULL
  DEFAULT 'local' CHECK (target_kind IN
    ('local','offsite_s3','offsite_rsync','offsite_b2','offsite_wasabi'));
ALTER TABLE backup_runs ADD COLUMN restore_scope_json TEXT;
```

## Endpoints

```
POST /api/v1/backups/runs
  body: { plan_id, restore_scope: RestoreScope, mode: "preview" | "execute",
          confirmed_at: <utc ts> }
POST /api/v1/backups/plans
  body: { name, schedules, resources, target_kind }
GET  /api/v1/backups/plans            filterable by target_kind
```

## Tests

```
1.1  Unit: ResourceKind/RestoreScope/TargetKind serde and
      validation; unknown kinds rejected.
1.2  Property: plans persisted with valid target_kind; restores
      require non-null restore_scope.
1.3  Service tests with mock remote: plan validates S3 bucket
      syntax; reject empty bucket names.
1.4  Integration: existing backup runs still pass.
```
