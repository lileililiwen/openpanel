# Refine software-center with web-application classification — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `WebApplicationManifest` parser and
      `SoftwareCatalog` lookups by kind.
- [x] 1.2 Property tests: every catalog entry has a stable id;
      manifests sign and verify under the master key.
- [ ] 1.3 Service tests with mock catalog: cross-kind install is
      rejected; same-kind install is accepted. (deferred)
- [ ] 1.4 Integration: existing system installs still pass;
      new column shows `system` on backfilled rows. (deferred)

## 2. Domain and Application

- [x] 2.1 Add `ComponentKind`, `TargetSiteType`,
      `WebApplicationManifest`, `CatalogEntry`,
      `check_install_kind` under
      `crates/openpanel-domain/src/software_center/refine.rs`.
- [ ] 2.2 Add SQLite migration: `kind` column + index. (deferred)
- [ ] 2.3 Extend `SoftwareCatalog` with two typed indices and
      `kind` discrimination on install. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Update `GET /software/catalog` JSON to include `kind`
      per item. (deferred)
- [ ] 3.2 Update CLI `openpanel software list` to flag kind.
      (deferred)
- [ ] 3.3 Add a tab on `/software` web page separating System
      components from Web applications. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: a backfilled row shows kind=system; a
      sample application manifest is listed but installing
      through system route is forbidden. (deferred)
- [x] 4.4 Archive with `openspec archive refine-software-center-with-app-classification`.
