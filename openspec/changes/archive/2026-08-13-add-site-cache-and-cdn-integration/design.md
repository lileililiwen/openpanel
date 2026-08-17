# Add site cache and CDN integration — Design

## Cache policy

```rust
pub struct SiteCachePolicy {
    pub site_id: SiteId,
    pub ttl_seconds: u32,                 // default 60
    pub bypass_paths: Vec<String>,        // e.g. ["/wp-admin", "/api/*"]
    pub static_assets_ttl_seconds: u32,   // default 7d
    pub keyed_cookies: Vec<String>,       // not used in cache key
    pub stale_while_revalidate: bool,
    pub revalidation_required: bool,
}
```

The `nginx` vhost snippet written by the sites cap is
augmented to:

```
proxy_cache_path /var/cache/openpanel/<site_id> levels=1:2 keys_zone=<z>:1m max_size=…
proxy_cache_key "$scheme$host$request_uri";
proxy_cache_valid 200 60s;
proxy_cache_valid 404 5s;
proxy_cache_bypass $cookie_admin_bypass;
```

## CDN adapter

```rust
#[async_trait]
pub trait CdnAdapter: Send + Sync {
    async fn list_zones(&self) -> Result<Vec<CdnZone>, CdnError>;
    async fn purge(&self, zone: CdnZone, urls: &[Url]) -> Result<PurgeReceipt, CdnError>;
    async fn set_cache_level(&self, zone: CdnZone, level: CacheLevel) -> Result<(), CdnError>;
    async fn get_headers(&self, zone: CdnZone) -> Result<HeaderSummary, CdnError>;
}
```

Concrete adapters:
- `CloudflareAdapter`: REST API against `/zones/:id/purge_cache`
  with typed bearer.
- `CloudFrontAdapter`: signed invalidation API.
- `GenericHttpAdapter`: send a POST to a configurable webhook
  with a typed payload.

## Endpoints

```
GET    /api/v1/sites/{id}/cache
PUT    /api/v1/sites/{id}/cache            body: SiteCachePolicy
POST   /api/v1/sites/{id}/cache/purge      body: { paths: [...] }
GET    /api/v1/cdn/integrations
POST   /api/v1/cdn/integrations            body: CdnIntegrationCreate
DELETE /api/v1/cdn/integrations/{id}
POST   /api/v1/cdn/purge                   body: { integration_id, paths }
```

## CLI

```
openpanel site cache show  <site_id>
openpanel site cache set   <site_id> --ttl <s> --bypass /wp-admin
openpanel site cache purge <site_id> --paths /article/x,/article/y
openpanel cdn integrations list
openpanel cdn purge        --integration <id> --paths /article/x
```

## Tests

```
1.1  Unit: nginx snippet generator; cache-key derivation;
      adapter contract.
1.2  Property: bypass_paths glob expansion is correct; cache
      key excludes listed cookies.
1.3  Service tests with mock adapter: list_zones, purge,
      set_cache_level; integration rejects unknown adapter.
1.4  Integration: live nginx cache policy + cache hit; live
      Cloudflare test mode for the adapter.
1.5  CLI E2E.
1.6  Web: cache editor with bypass editor (CSRF), purge dialog.
```
