# Add Container image registry — Tasks

## 1. Testing

- [x] 1.1 Unit: retention prune by count and by age; namespace quota
      enforcement on push overflow.
- [x] 1.2 Property: image blobs are stored only under the owning
      namespace path; the scan hook runs whenever `scan_on_push` is set.
- [x] 1.3 Service: push stores an image; scan produces a `ScanResult`;
      retention prunes oldest/over-count images.
- [x] 1.4 Integration: a real `docker push` succeeds via the OCI
      Distribution routes; a push to another user's namespace returns
      `403`.
- [x] 1.5 Web: Registry tab (CSRF), namespace list, image scan badges.

## 2. Domain and Application

- [x] 2.1 Implement `RegistryConfig`, `ImageNamespace`, `StoredImage`,
      `ScanResult` under
      `crates/openpanel-domain/src/container_registry/`.
- [x] 2.2 Add SQLite migration for `registry_config`, `image_namespaces`,
      `stored_images`, `scan_results`.
- [x] 2.3 Implement `RegistryService`, `RetentionPolicy`, `ScanHook`;
      register via `ModuleRegistry`; wire to `container-runtime`.

## 3. Adapters and UI

- [x] 3.1 Add `/registry/config`, `/registry/namespaces`,
      `/registry/images` REST routes plus OCI Distribution push/pull.
- [x] 3.2 Build the Registry tab (CSRF), namespace management, scan
      badges.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (clippy pre-existing).
- [x] 4.3 Smoke-test: create a namespace, `docker push` an image,
      confirm a scan result; overflow quota and confirm rejection.
- [x] 4.4 Archive with `openspec archive add-container-image-registry`.