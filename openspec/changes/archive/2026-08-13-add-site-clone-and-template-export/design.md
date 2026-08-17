# Add site clone and template export — Design

## Sources

```rust
pub enum CloneSource {
    Site(SiteId),
    Snapshot(SiteId, snapshot_id),
    Template(TemplateId),
}
```

## Plan and run

```
clone(source, target_domain, target_owner_id, options):
  plan = ClonePlan {
    files: [{ src, dst, content_hash, exclude: ["node_modules", ".git", ...] }],
    db:   { create: true|false, source_dump_size_est, anonymise: bool },
    overwrites: [],
    warnings: []
  }
  run only after confirmed_at ±60s.
  - rsync / extract under site chroot
  - dump + load DB
  - anonymise users unless keep_pii=true
  - rewrite Site.url + canonical URLs (config_overlay approach)
  - persist new Site with instance_origin_id = source
  - audit SiteCloned{from, to, policy}
```

## Template export

```
export-template(site_id):
  chroot = site.document_root
  tar -C <parent> --exclude-from <patterns> -cf /tmp/template.tar.gz
  template.json = { site_meta, file_facts, db_schema_summary,
                    config_overlay_paths, anonymisation_tokens,
                    signature: <ed25519 over template.json> }
  write tar + template.json -> template_artifact_dir
  persist SiteTemplate{id, name, version, artifact_path, signature}
```

## PII

Anonymisation policy `"standard"`:
- `users.email` → `<uuid>@template.local`
- `users.password_hash` → reset
- Replacement is recorded per-row in `anonymisation_tokens`
  (encrypted under master key); can be reversed only with the
  template id's KEK.

## Endpoints

```
POST /api/v1/sites/{id}/clone
  body: CloneRequest { source, target_domain, target_owner_id,
                       keep_pii?: bool, dry_run?: true }

POST /api/v1/sites/{id}/export-template
  body: { name, description, anonymisation_policy }
GET  /api/v1/sites/templates                 list + filter
POST /api/v1/sites/templates/{tid}/clone-to body: target_domain, target_owner_id
```

## CLI

```
openpanel site clone            <site_id> --target-domain <d>
                                      --target-owner <u>
                                      [--keep-pii]
openpanel site export-template  <site_id> --name <n>
openpanel site template-list
openpanel site template-clone   <template_id> --target-domain <d>
```

## Tests

```
1.1  Unit: chroot validation; PII anonymisation rule;
      template signature verify; overwrite detector.
1.2  Property: clone never imports files outside the source
      chroot; anonymisation is idempotent and reversible only
      with the right KEK.
1.3  Service tests with mock filesystem and DB: clone from
      site, snapshot, template; export-template round-trip.
1.4  Integration: clone a test fixture site end-to-end; export
      then re-import.
1.5  CLI E2E.
1.6  Web: clone wizard, export-template dialog (CSRF), template
      list with thumbnails.
```
