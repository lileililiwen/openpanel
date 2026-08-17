# Refine sites with multi-PHP runtime, clone, and template export — Design

## New fields

```rust
pub struct Site {
    // ... existing fields ...
    pub php_runtime: Option<PhpRuntimeRef>,        // v0.2
    pub clone_template_id: Option<Uuid>,
    pub instance_origin_id: Option<Uuid>,
}
```

`PhpRuntimeRef` is a thin reference — `{ package_id, version, socket_path }` —
to a `Software` row managed by the `software-center`. The reference
is empty in v0.1; the follow-on `add-per-site-php-runtime` change
populates it.

## Migration

```sql
ALTER TABLE sites ADD COLUMN php_runtime_json TEXT;
ALTER TABLE sites ADD COLUMN clone_template_id TEXT;
ALTER TABLE sites ADD COLUMN instance_origin_id TEXT;
CREATE INDEX idx_sites_clone_template_id ON sites(clone_template_id);
CREATE INDEX idx_sites_instance_origin_id ON sites(instance_origin_id);
```

All three columns nullable; pre-existing rows backfill with NULL.

## Clone (placeholder)

```rust
impl Site {
    pub fn clone(source: &Site, target_domain: DomainName, new_owner_id: UserId) -> DraftSite {
        DraftSite {
            primary_domain: target_domain,
            document_root: source.document_root.rebase(target_domain),
            php_runtime: source.php_runtime.clone(),
            instance_origin_id: Some(source.id),
            ..DraftSite::defaults()
        }
    }
}
```

The draft is not persisted by this change. The follow-on change
makes it persistable.

## Tests

```
1.1  Unit: Site::clone preserves fields, rebases document_root;
      fields are persisted round-trip.
1.2  Property: site rows from migration backfill with NULL on
      every new column; no test site row is missing a primary key.
1.3  Service tests: round-trip through repository; clone draft
      serialises to JSON exactly like a Site minus id/timestamps.
1.4  Integration: existing site tests pass; new columns persist.
```
