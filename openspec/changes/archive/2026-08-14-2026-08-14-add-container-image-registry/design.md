# Add Container image registry — Design

## ImageNamespace model

```rust
pub struct ImageNamespace {
    pub namespace_id: NamespaceId,
    pub owner: UserId,             // tenant isolation
    pub quota_bytes: u64,
    pub used_bytes: u64,
}
```

## RegistryConfig

```rust
pub struct RegistryConfig {
    pub storage_root: PathBuf,
    pub retention: RetentionPolicy, // { max_images_per_ns, max_age_days }
    pub scan_on_push: bool,
}
```

## Push / retention / scan flow

```
push(user, namespace, manifest+blobs):
  authz: user owns namespace (else 403)
  write via OCI Distribution layout under storage_root/<ns>/
  if scan_on_push: ScanHook.scan(image) -> ScanResult
      store ScanResult; do NOT auto-delete on findings
  RetentionPolicy.apply(ns): prune oldest/over-count images
  audit RegistryImagePushed{ns, digest, scan_status}

config GET/PUT /registry/config:
  read/update RegistryConfig (admin-scoped)
```

## Endpoints

```
GET  /api/v1/registry/config
PUT  /api/v1/registry/config         body { retention?, scan_on_push? }
POST /api/v1/registry/namespaces     body { owner, quota_bytes? }
POST /api/v1/registry/images         body { namespace, digest, manifest }
      (and the OCI Distribution push/pull routes behind panel auth)
```

## Tests

```
1.1 Unit: retention prune (count + age); namespace quota enforcement.
1.2 Property: blobs stored only under the owner's namespace path;
    scan hook always runs when scan_on_push is set.
1.3 Service tests w/ mock storage: push, scan, prune.
1.4 Integration: docker push via OCI routes; unauthorised ns -> 403.
1.5 Web: Registry tab (CSRF), namespace list, scan badges.
```
