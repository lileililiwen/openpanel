# Add container runtime — Tasks

## 1. Testing

- [x] 1.1 Unit tests: quota math; egress accounting; credential
      cipher round-trip.
- [x] 1.2 Property tests: quota monotonic; passwords zeroized.
- [x] 1.3 Service tests with mock docker and adapter.
- [x] 1.4 Integration: live docker fixture; pull against test
      registry.
- [x] 1.5 CLI E2E.
- [x] 1.6 Web: `/container/quota` and `/registry/credentials`.

## 2. Domain and Application

- [x] 2.1 Add `ContainerQuota`, `RegistryCredential`,
      `ContainerMetrics`, `NetworkEgressAccount` under
      `crates/openpanel-domain/src/container_runtime/`.
- [x] 2.2 Add SQLite migration for `container_quotas`,
      `registry_credentials`, `network_egress_accounts`.
- [x] 2.3 Implement `ContainerQuotaService`, `RegistryAdapter`,
      metrics poller, egress observer.

## 3. Adapters and UI

- [x] 3.1 Add the REST routes.
- [x] 3.2 Add the CLI subcommands.
- [x] 3.3 Build the web pages (CSRF).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: enforce a quota; try a 9th create;
      pull an image against a stub registry.
- [x] 4.4 Archive with `openspec archive add-container-runtime`.
