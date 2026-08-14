# Refine plugin extension framework with marketplace — Design

## Catalog model

```rust
pub struct MarketplacePlugin {
    pub id: String,
    pub name: String,
    pub publisher: String,        // verified against CA
    pub manifest_url: String,     // pinned by catalog entry
    pub rating: f32,              // 0.0..=5.0
    pub summary: String,
}
```

## Flow

```
discover():
  fetch remote catalog (ETag / If-Modified-Since) -> cache locally
  verify each entry's publisher signature against marketplace CA
  entries failing verification are flagged, never auto-installed

install_from_marketplace(id):
  resolve manifest_url (pinned by catalog entry)
  verify manifest signature chain -> marketplace CA
  delegate to existing PluginRegistry.install (capability gating)
  audit PluginInstalledFromMarketplace{publisher, id}
```

## Endpoints

```
GET  /api/v1/marketplace/plugins
GET  /api/v1/marketplace/plugins/{id}
POST /api/v1/marketplace/plugins/{id}/install
```

## Tests

```
1.1 Unit: signature verify (valid/invalid CA); catalog cache parse.
1.2 Property: only CA-chained publishers appear installable.
1.3 Service: discover, install delegates to framework, rollback.
1.4 Integration: install from marketplace reaches PluginRegistry.
1.5 CLI E2E: marketplace list -> install.
1.6 Web: marketplace tab (CSRF), install button.
```
