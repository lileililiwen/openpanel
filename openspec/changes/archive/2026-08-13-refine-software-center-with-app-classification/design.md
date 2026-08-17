# Refine software-center with web-application classification — Design

## Manifest schemas

`system-component.json` (existing content unchanged) is augmented
with `"kind": "system"` at the top level for clarity.

`web-application.json` adds the schema:

```jsonc
{
  "kind": "application",
  "id": "wordpress",
  "display_name": "WordPress",
  "version": "6.6.2",
  "category": "cms",
  "license": "GPL-2.0",
  "target_site_type": "single_site",
  "min_php": "8.1",
  "requires_db": true,
  "config_overlay_paths": ["wp-config.php"],
  "post_install_url": "/wp-admin/install.php",
  "checksum_pem": "<public key>",
  "artifacts": [
    {
      "url": "https://downloads.wordpress.org/release/wordpress-6.6.2.tar.gz",
      "sha256": "…",
      "signature_url": "…"
    }
  ]
}
```

## Catalog indices

```rust
pub struct SoftwareCatalog {
    system: BTreeMap<ComponentId, SystemComponentManifest>,
    applications: BTreeMap<AppId, WebApplicationManifest>,
    by_category: BTreeMap<Category, Vec<ComponentId>>,
}
```

Install endpoints dispatch to `SoftwareInstaller` with explicit
`kind`. Cross-kind installs (`install_application_through_system`)
are rejected with `SoftwareError::CrossKindInstallForbidden`.

## Migration

```sql
ALTER TABLE installed_components ADD COLUMN kind TEXT NOT NULL
  CHECK (kind IN ('system','application')) DEFAULT 'system';
CREATE INDEX idx_installed_components_kind ON installed_components(kind);
```

## Tests

```
1.1  Unit: manifest parsers; kind dispatch; cross-kind guard.
1.2  Property: every catalog entry has a stable id; manifests
      are signature-verifiable.
1.3  Service tests with mock catalog: install_system_with_app_id
      is rejected with typed error; install_application with
      system-id is rejected.
1.4  Integration: existing system installs still pass; new
      application manifests are listed but cannot be installed
      by this change (installer is the follow-on change).
```
