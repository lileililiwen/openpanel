# Tasks: docker-backed delivery and deployment adapters

## 1. Testing

- [x] Unit `DockerBuildAdapter`: git→build→push happy path yields
  digest-pinned `ReleaseDigest`; OCI source verifies digest presence;
  daemon-down and auth-reject map to chosen existing domain error variants
  with fixed redacted strings; table-driven.
  (`docker_build_tests.rs`, 9 tests: OCI present / OCI pullable /
  daemon-down typed+redacted / build-push-digest with the build command
  preserved in the call log / auth rejection redacted / credentialed URL
  refused before the engine / context ceiling / invalid namespace.)
- [x] Unit `DockerRuntimeAdapter`: deploy converges container with the
  three labels; healthy-vs-running policy honored; rollback to missing
  target returns typed error; diagnostics bounded (no env/creds).
  (`docker_runtime_tests.rs`, 13 tests: three labels + digest-pinned
  image / pull when absent / unhealthy verdict over running status /
  healthy verdict / running-without-healthcheck accepted / exited is
  terminal / daemon typed+redacted / rollback to unknown release inert /
  rollback re-converges / cross-environment rollback refused / bounded
  diagnostics / mutable digest refused / read-only check / invalid
  namespace.)
- [x] Unit `DockerDeploymentAdapter`: plan/manifest id mismatch refused;
  dry-run records zero mutations (docker double call log assertion);
  replay with cached evidence returns it unchanged.
  (`deployment_adapters/docker_adapter_tests.rs`, 10 tests, incl. the
  three scenarios plus manifest id + dry-run capability + unresolvable
  target + out-of-namespace target + daemon outage + read-only status.)
- [x] Integration: facade composition test — `ForgePublishProvider` with
  real adapters over the docker double; Preflight then Deploy succeed
  where the Unavailable seam previously failed.
  (`tests/integration/application_delivery.rs`, 6 new tests, incl. the
  `dry_run` → `succeeded` publish sequence and a contrast test proving an
  unregistered adapter is still refused.)
- [x] E2E: extend `tests/cli/forge_provider.rs` — publish frame now returns
  success status against the double-backed harness (document how the
  harness injects the double, e.g. test-only composition fn).
  (`crates/openpanel-cli/tests/cli/forge_provider.rs`:
  `cli_forge_provider_publish_frame_succeeds_against_the_double` drives the
  production `run_loop` over `double_harness`; the injection point is
  documented in the test's doc comment — a test-only composition function
  in the app crate, not an env switch in the shipped binary.)

## 2. Implementation

- [x] Inventory first: exact docker-service abstraction the double must
  implement, the git-clone helper used by site git-deploy, and the domain
  error variant table for unavailable/auth cases (record choices before
  coding; STOP rule if none fits).
  (Recorded in `design.md` § inventory + § Implementation rulings; the
  domain error variants are reused, none added:
  `RuntimeUnavailable` / `RegistryUnauthorized` / `MutableDigest` /
  `Invalid` / `IllegalTransition` / `ReleaseNotReady`.)
- [x] `application_delivery/docker_build.rs` + `docker_runtime.rs` per
  design contract; registry push via `container_registry` reuse.
  (Push is `BollardDockerAdapter::push_image` with the port's existing
  `registry_credentials` seam; the namespace is config-driven rather than a
  new `container_registry` binding, so the push path stays inside the
  docker module the build adapter already reuses.)
- [x] `deployment_adapters/docker_adapter.rs` with manifest + execute
  semantics.
- [x] Swap `forge_provider` composition to real adapters; delete
  `forge_publish_provider/unavailable.rs` and its exports.
  (`crates/openpanel-cli/src/forge_provider.rs` composes one shared
  `DockerRuntimeAdapter` behind all three adapters; the file is deleted and
  the lib/module doc comments updated.)

## 3. Regression

- [x] Delivery/deployment service behavior unchanged for existing callers
  (`make application-delivery`, `make deployment-adapters` check targets
  green; their suites unmodified).
  (Both targets green. No `service.rs` in either bounded context was
  touched; `DeploymentAdapterService` keeps its synthetic dry-run path —
  see `design.md` ruling 1.)
- [x] `forge-publish-provider-stdio` requirements still hold (transport
  tests green; only composition changed).
  (`transport_tests.rs` now composes the real adapters over the shared
  double; only the deleted `Unavailable` marker assertions were removed.
  The CLI subprocess tests are unchanged and still green.)
- [x] `scripts/check-reuse.sh --strict` + baseline classification for the
  three new type names; `make file-length` on new modules.
  (Six new accessor names classified in
  `openspec/governance/.reuse-classified-baseline`
  (`application_id`, `environment_id`, `release_id`, `digest`, `labels`,
  `with_health`); `make reuse-strict` green. New modules are well under the
  soft limit; `tests/integration/deployment_adapters.rs` grew past the soft
  limit and stays under the hard limit.)
- [x] Domain impact is explicit and verified: `BuildAdapter::resolve_digest`
  receives the unchanged build command; new typed error variants are mapped
  by the facade; existing fake adapters and domain tests remain green.
  (No new domain error variant: the four the change needed already existed.
  The build command is threaded through the port unchanged and asserted in
  the build-adapter call log. The facade maps `Domain(_)` to the existing
  adapter error, asserted by the registry-auth test. Existing fakes and
  domain tests green.)

## 4. Verification

- [x] `cargo fmt`; `cargo clippy --all-targets -- -D warnings`;
  `cargo test` (workspace + targeted crates).
- [x] `openspec validate --all --strict --no-interactive`; human approval
  of design.md before apply.
  (`openspec validate application-delivery-docker-adapters --strict` →
  "Change 'application-delivery-docker-adapters' is valid". The design
  correction was approved by the principal before the apply phase, per
  `progress.md`.)
- [x] `make check` chain + `make gate`; HANDOFF two-commit cadence; release
  note that cross-process durability still awaits
  `application-delivery-sqlite-state` (evidence boundary stated).
  (Evidence boundary stated in `design.md` § Evidence boundary: rollback
  durability is per-process until `application-delivery-sqlite-state`
  lands.)
