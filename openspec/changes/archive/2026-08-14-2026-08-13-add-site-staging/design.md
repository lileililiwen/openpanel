# Add site staging — Design

## Slot model

```rust
pub struct StagingSlot {
    pub site_id: SiteId,
    pub subdomain: String,                 // "staging" by default
    pub document_root: PathBuf,            // /var/www/<d>/staging/public_html
    pub db_name: String,                    // {owner}_{site}_staging
    pub php_runtime: Option<PhpRuntimeRef>, // mirrors prod
    pub sync_policy: SyncPolicy,           // OnPromote | OnDemand | Scheduled
    pub promotion: PromotionStatus,         // Idle | Pending | Promoted
}
```

## Filesystem layout

```
/var/www/<primary_domain>/
    public_html/                prod
    staging/
        public_html/            staging
        .snapshots/             prior sync snapshots
```

The staging docroot is mounted read-write to the site owner
within the chroot rules. The staging DB is a clone of the prod
DB created on first sync, anonymised by default (per
`add-site-clone-and-template-export`).

## Sync

```
sync(site_id, mode):
  mode ∈ {Snapshot, Promote}
  Snapshot:
    rsync prod docroot → staging docroot; pending atomic move.
    mysqldump prod_db → staging_db; pending pg transfer
    staging.snapshot_id += 1
  Promote:
    run after destroy the chosen snapshot; raise PromotionStatus = Pending.
```

## Promote (atomic)

```
promote(site_id, snapshot_id, confirmed_at):
  verify within 60s
  chmod 0500 staging.public_html (lock writes)
  rename(staging.public_html, prod.public_html.next)
  rename(prod.public_html, prod.public_html.prev)
  rename(prod.public_html.next, prod.public_html)
  nginx -t && nginx -s reload
  if any step fails: rollback, audit PromotionRolledBack
  else: PromotionStatus = Promoted
```

The rollback window is 60 seconds; after that the rename
chain is committed.

## Endpoints

```
POST /api/v1/sites/{id}/staging/create
POST /api/v1/sites/{id}/staging/sync    body: { mode: "snapshot" | "promote" }
POST /api/v1/sites/{id}/staging/promote body: { snapshot_id, confirmed_at }
DELETE /api/v1/sites/{id}/staging
```

## Tests

```
1.1  Unit: chroot validation; staging DB naming; promotion
      transaction ordering.
1.2  Property: staging docroot cannot escape /var/www/<d>;
      sync is idempotent for the same snapshot id.
1.3  Service tests with mock filesystem: snapshot, promote,
      rollback, lock.
1.4  Integration: live promote across nginx; rollback on
      failure.
1.5  CLI E2E: create → sync → promote → destroy.
1.6  Web: staging tab (CSRF), promote confirmation.
```
