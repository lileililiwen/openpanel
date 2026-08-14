# Add Container image registry

## Why

`container-runtime` (active) adds registry credentials and egress for
pulling external images, but the panel hosts **no registry of its own**
— teams cannot push, store, and pull their own images through OpenPanel.
This change adds a `container-registry` bounded context: a panel-hosted
OCI Distribution-compliant image registry with per-user namespaces,
push/pull through the panel, a retention policy, and a vulnerability-scan
hook invoked after each push.

## What Changes

- New bounded context `container-registry` carrying the `RegistryConfig`,
  `ImageNamespace`, and `StoredImage` aggregates, plus
  `RegistryService`, `RetentionPolicy`, `ScanHook`.
- New endpoints: `GET/PUT /api/v1/registry/config`,
  `POST /api/v1/registry/namespaces`,
  `POST /api/v1/registry/images` (push hook).
- A panel-hosted registry implementing the OCI Distribution spec, so
  standard `docker push/pull` clients work against it.
- Per-user namespaces isolating image storage; push/pull authenticated
  via the panel identity.
- A retention policy (count/age based) and a vulnerability-scan hook
  triggered on push.

## Capabilities

### New Capabilities

- `container-registry`: a panel-hosted OCI Distribution registry with
  per-user namespaces, push/pull via the panel, a retention policy, and
  a post-push vulnerability-scan hook.

## Impact

- Domain: `RegistryConfig`, `ImageNamespace`, `StoredImage`,
  `ScanResult`.
- App: `RegistryService`, `RetentionPolicy`, `ScanHook`.
- API/CLI/web: `/registry/config`, `/registry/namespaces`,
  `/registry/images`; web Registry tab (CSRF).
- Security: registry auth via panel identity; namespaces isolate
  tenants; scan results gate promotion but do not auto-delete images.
- Coupling: depends on `container-runtime` for the underlying runtime
  and credentials; integrates the scan hook with
  `add-web-application-malware-scanner` for the vulnerability feed.
