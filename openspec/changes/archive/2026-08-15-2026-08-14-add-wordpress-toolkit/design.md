# Add WordPress toolkit — Design

## WpSite model

```rust
pub struct WpSite {
    pub site_id: SiteId,
    pub wp_root: PathBuf,         // inside site chroot
    pub version: String,          // detected WP core version
    pub cache_mode: WpCacheMode,  // Off | Object | Page
}

pub struct WpUpdateSet {
    pub core: Option<String>,     // target version
    pub plugins: Vec<String>,     // slugs
    pub themes: Vec<String>,      // slugs
}

pub struct WpSecurityReport {
    pub perms_ok: bool,
    pub outdated: Vec<String>,    // core/plugin/theme names
    pub known_vulns: Vec<String>, // CVE ids from local feed
}
```

## Staging / clone flow

```
stage(site_id):
  target = site_staging.create(site_id)         // reuse site-staging
  copy wp_root + clone DB into target
  rewrite wp-config (new table prefix / URL)
  audit WpStaged{target_site_id}

clone(site_id, target_name):
  site_clone.export_and_import(site_id, target_name)  // reuse clone cap
  audit WpCloned{target_site_id}
```

## Update + rollback flow

```
update(site_id, set):
  snapshot = db.snapshot() + tar wp_root
  apply core/plugin/theme updates
  run wp health check
  on failure: restore snapshot + wp_root; audit WpUpdateRolledBack
  on success: audit WpUpdated{set} (names only)
```

## Security scan + cache

```
scan(site_id):
  perms_ok = check_file_perms(wp_root)   // read-only
  outdated = compare versions vs latest
  known_vulns = match local CVE feed
  return WpSecurityReport

set_cache(site_id, mode):
  write wp-config cache constants
  flush caches
  audit WpCacheChanged{mode}
```

## Endpoints

```
POST /api/v1/sites/{id}/wordpress/staging
POST /api/v1/sites/{id}/wordpress/clone     body { target_name }
POST /api/v1/sites/{id}/wordpress/updates   body { core?, plugins?, themes? }
GET  /api/v1/sites/{id}/wordpress/security
PUT  /api/v1/sites/{id}/wordpress/cache     body { mode }
```

## Tests

```
1.1 Unit: version compare; CVE feed match; cache-constant render;
        rollback restores snapshot.
1.2 Property: staging/clone target stays inside panel-managed space;
        update is reversible.
1.3 Service w/ mock staging+clone+DB: stage, clone, update+rollback,
        scan, cache.
1.4 Integration: live WP stage; broken update rolls back; scan flags
        outdated + CVE.
1.5 CLI E2E: `openpanel site wp stage` -> `update` -> `scan`.
1.6 Web: WP tab (CSRF), stage/clone buttons, update panel, scan
        report, cache toggle.
```
