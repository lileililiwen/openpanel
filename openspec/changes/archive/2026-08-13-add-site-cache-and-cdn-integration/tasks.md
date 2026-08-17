# Add site cache and CDN integration — Tasks

## 1. Testing

- [x] 1.1 Unit tests: nginx snippet generator; cache-key
      derivation; adapter contract.
- [x] 1.2 Property tests: bypass path globs; cookie exclusions.
- [x] 1.3 Service tests with mock adapter; integration rejects
      unknown adapter.
- [x] 1.4 Integration: live nginx cache hit; live Cloudflare
      test mode.
- [x] 1.5 CLI E2E.
- [x] 1.6 Web: cache editor (CSRF), purge dialog.

## 2. Domain and Application

- [x] 2.1 Add `SiteCachePolicy`, `CdnIntegration`, `CdnZoneRef`,
      `PurgeRequest` under
      `crates/openpanel-domain/src/site_cache_cdn/`.
- [x] 2.2 Add SQLite migration for `site_cache_policies`,
      `cdn_integrations`, `cdn_purge_log`.
- [x] 2.3 Implement `SiteCacheService`, `CdnAdapterRegistry`,
      Cloudflare / CloudFront / Generic HTTP adapters.

## 3. Adapters and UI

- [x] 3.1 Add the REST routes.
- [x] 3.2 Add `openpanel site cache {show,set,purge}`,
      `openpanel cdn {integrations,purge}`.
- [x] 3.3 Build the cache editor (CSRF) and purge dialog.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [ ] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: configure cache on a test site; verify
      nginx vhost snippet; run a purge against a mock CDN
      adapter.
- [x] 4.4 Archive with `openspec archive 2026-08-13-add-site-cache-and-cdn-integration`.
