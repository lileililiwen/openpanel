# Refine plugin extension framework with marketplace — Tasks

## 1. Testing

- [x] 1.1 Unit: publisher signature verify (valid/invalid CA);
      catalog cache parse round-trip.
- [x] 1.2 Property: only CA-chained publishers are installable;
      a tampered catalog entry is rejected.
- [x] 1.3 Service: discover, install delegates to the framework
      install flow, audit recorded; failure rolls back.
- [x] 1.4 Integration: install from marketplace reaches
      `PluginRegistry`; capability gates still apply.
- [x] 1.5 CLI E2E: `openpanel plugin marketplace list` -> `install`.
- [x] 1.6 Web: marketplace tab (CSRF), install button + status.

## 2. Domain and Application

- [x] 2.1 Extend the `plugin-extension-framework` domain with
      `MarketplaceCatalog`, `PublisherSignature`, `PluginRating`.
- [x] 2.2 Add a cache for the catalog (SQLite or file cache under
      the panel data dir).
- [x] 2.3 Implement `MarketplaceClient` reusing
      `PluginRegistry` / `PluginSupervisor` for install.

## 3. Adapters and UI

- [x] 3.1 Add `/marketplace/plugins*` REST routes.
- [x] 3.2 Add `openpanel plugin marketplace {list,install}`.
- [x] 3.3 Build the marketplace tab (CSRF).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (clippy pre-existing).
- [x] 4.3 Smoke-test: configure a marketplace CA, list plugins,
      install one; confirm the publisher signature is verified.
- [x] 4.4 Archive with `openspec archive
      refine-plugin-extension-framework-with-marketplace`.