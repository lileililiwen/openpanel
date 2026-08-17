# Add site cache and CDN integration

## Why

`openspec/specs/sites/spec.md` covers vhost provisioning; it
does not pin a **page-cache layer** or a **CDN integration**.
cPanel ships LiteSpeed cache rules and Cloudflare
auto-purge; Baota ships Nginx microcaching and a one-click
Cloudflare sync. Operators expect both. This change adds a
typed cache policy per site plus a Cloudflare-compatible CDN
adapter that supports zone list, purge, and cache-level
configuration.

## What Changes

- New bounded context `site-cache-cdn` carrying the `SiteCachePolicy`
  and `CdnIntegration` aggregates.
- New endpoints: `GET/PUT /sites/{id}/cache`,
  `GET/POST/DELETE /cdn/integrations`,
  `POST /cdn/purge`.
- New SQLite tables: `site_cache_policies`, `cdn_integrations`,
  `cdn_purge_log`.
- Cache layer: nginx `proxy_cache_path` with TTL + key tuples
  per site; FastCGI cache for PHP sites; static asset cache.
- CDN adapter interface: `CdnAdapter` trait with
  `list_zones`, `purge`, `set_cache_level`, `get_headers`.

## Capabilities

### New Capabilities

- `site-cache-cdn`: per-site cache policy and CDN integration.

## Impact

- Domain: `SiteCachePolicy`, `CdnIntegration`, `CdnZoneRef`,
  `PurgeRequest`.
- App: `SiteCacheService`, `CdnAdapterRegistry`.
- API/CLI/web: `/sites/{id}/cache`, `/cdn/*`; CLI
  `openpanel site cache {get,set}`,
  `openpanel cdn {integrations,purge}`; web `/sites/cache`.
- Coupling: depends on `sites` for chroot and on `ssl` for
  CDN-issued certs.
