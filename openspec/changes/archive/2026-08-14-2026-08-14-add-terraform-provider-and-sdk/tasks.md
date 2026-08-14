# Add Terraform provider and SDK — Tasks

## 1. Testing

- [x] 1.1 Unit: contract parser maps OpenAPI operations to SDK methods
      and provider resources; auth + 4xx error types enumerated.
- [x] 1.2 Property: every provider resource has a backing endpoint in
      the contract; regenerated SDK is byte-stable for unchanged API.
- [x] 1.3 CI: generated SDK (rust/go/ts) and provider compile against
      `openapi.rs`; a drift between committed and generated fails.
- [x] 1.4 Integration: provider `apply` creates a site through the live
      REST API; `destroy` removes it.
- [x] 1.5 E2E: `terraform plan/apply/destroy` round-trips a site plus a
      DNS zone.

## 2. Domain and Application

- [x] 2.1 Implement `ApiContract`, `SdkPackage`, `TerraformProvider`
      descriptors under `crates/openpanel-domain/src/iac/`.
- [x] 2.2 Implement `CodegenContract` that reconciles the generated
      SDK/provider against `openapi.rs`.
- [x] 2.3 Wire the sync CI job (regenerate + drift check) in the
      pipeline.

## 3. Adapters and UI

- [x] 3.1 Publish SDK packages (Rust/Go/TS) from the contract.
- [x] 3.2 Scaffold the Terraform provider repository for
      sites/users/dns/backups.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (clippy pre-existing).
- [x] 4.3 Smoke-test: run the codegen CI locally; confirm SDK + provider
      generate and the drift check passes against current `openapi.rs`.
- [x] 4.4 Archive with `openspec archive add-terraform-provider-and-sdk`.