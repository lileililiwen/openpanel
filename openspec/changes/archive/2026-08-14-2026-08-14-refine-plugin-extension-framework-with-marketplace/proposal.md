# Refine plugin extension framework with marketplace

## Why

`add-plugin-extension-framework` defines a signed recipe protocol,
a local `PluginRegistry`, and a `/plugins` install/enable/disable
flow, but it has **no remote discovery layer**: there is no curated
catalog, no publisher signing verification against a marketplace CA,
and no ratings/metadata. Operators must hand-supply a manifest URL.
cPanel's plugin ecosystem and Baota's plugin store both ship a
marketplace. This change adds the remote catalog / discovery layer
on top of the existing framework; it does not replace the local
registry.

## What Changes

- New remote catalog surface in the `plugin-extension-framework`
  cap: a curated registry (remote catalog + local cache),
  discover / install / update from the marketplace, publisher
  signature verification against a marketplace CA, and
  ratings/metadata.
- New endpoints: `GET /marketplace/plugins`,
  `POST /marketplace/plugins/{id}/install` (reuses the framework
  install flow), `GET /marketplace/plugins/{id}`.
- Marketplace CA public key configured globally; plugin manifests
  MUST chain to it or install is refused.

## Capabilities

### Modified Capabilities

- `plugin-extension-framework`: remote catalog / marketplace
  discovery, publisher CA verification, ratings/metadata.

## Impact

- Domain: `MarketplaceCatalog`, `PublisherSignature`,
  `PluginRating` (extensions to the plugin cap).
- App: `MarketplaceClient`, `CatalogCache`; reuses
  `PluginRegistry` / `PluginSupervisor` for install.
- API/CLI/web: `/marketplace/plugins*`; CLI
  `openpanel plugin marketplace {list,install}`; web marketplace tab.
- Security: publisher signatures verified against the marketplace
  CA before any install; cache integrity checked on read.
- Coupling: depends on `plugin-extension-framework` for the install
  protocol and capability gating.
