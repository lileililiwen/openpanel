# Docker adapters implementation progress

## 2026-10-01: inventory and domain port phase

- Original active folder was untracked; no existing implementation edits were present.
- Ran `scripts/repo-map.sh`, read normative agent contract, config, agent-quality
  spec, domain/app ports, Docker/Bollard implementation, provider composition,
  registry service, and deployment service. Revised `design.md` and delta spec
  to record the actual build/push, git clone, config, error, and dry-run gaps.
- Ruling: explicit BuildAdapter build-command argument and typed delivery errors
  are needed; parent authorized design correction to satisfy DCK-001..004.
- Red: `rtk cargo test -p openpanel-app
  test_build_release_passes_build_command_unchanged --lib` failed E0050 because
  the port lacked the build-command argument. Green: same focused test passed
  after port, service call, and fake updates (1 passed).
- Red: `rtk cargo test -p openpanel-domain
  test_runtime_and_registry_errors_are_typed_and_redacted --lib` failed E0599
  because typed variants were absent. Green: same focused test passed after
  adding variants and facade mapping (1 passed).
- Remaining: Docker port and Bollard build/push/inspect/labeled converge,
  application build/runtime adapters, deployment adapter, provider composition,
  all new unit/integration/CLI tests, regression gates, strict validation,
  archive, and the two related-only commits. No runtime verification claim yet.

## 2026-10-02: adapters, composition, and the full verification chain

Implementation landed in this order, each step red-then-green:

- Docker port: added `build_image`, `push_image`, `image_exists`,
  `create_labeled`, plus `RuntimeContainerState::health` so the
  healthy-vs-running policy is real. `pull`, `ping`, and `inspect` were
  extended the same way. Focused clippy/test runs green after each.
- `DockerBuildAdapter` (`application_delivery/docker_build.rs`): 9 unit tests
  over the shared double, including a real local git fixture (the fixture
  repository enables `uploadpack.allowAnySHA1InWant` because a forge host that
  serves a commit by sha is what a `file://` fixture must emulate). The fixture
  `TempDir` is returned to the test so the source repository outlives the fetch.
- `DockerRuntimeAdapter` (`docker_runtime.rs`): 13 unit tests. The engine I/O
  runs on a dedicated worker thread (`converge_blocking`) because the domain
  ports are synchronous and a provider frame already runs inside Tokio.
- `DockerDeploymentAdapter` (`deployment_adapters/docker_adapter.rs`): 10 unit
  tests; dry-run, replay, and manifest-mismatch are asserted through the double's
  call log.
- `UnavailableBuildAdapter` / `UnavailableRuntimeAdapter` deleted, their exports
  and doc comments removed; the transport suite now composes the real adapters.
- CLI composition swapped; delivery settings read from the generic
  `Config.modules.delivery` map (namespace, context ceiling, optional push
  credentials, declared plan secret refs).
- One recording docker double serves every suite
  (`docker/testing.rs`), plus `application_delivery::testing` for the delivery
  port double and `forge_publish_provider::testing::double_harness` for the
  full graph. It lives in `openpanel-app` (not `openpanel-test-support`)
  because a double in a crate that depends on `openpanel-app` cannot satisfy
  the trait the unit tests see.
- Integration: 6 new application-delivery tests (pinned digest, auth
  redaction, labeled converge, typed inert rollback, `dry_run` → `succeeded`
  publish, and a contrast test that an unregistered adapter is still refused)
  and 4 new deployment-adapter tests carrying the three DCK-003 fingerprints.
- E2E: the CLI suite drives the production `run_loop` over the double-backed
  harness and asserts `dry_run` then `succeeded`. No shipped code path reads a
  "pretend to publish" switch, so the subprocess tests keep proving the real
  entry point.

Four rulings were forced by contracts this change does not own; they are
recorded in `design.md` § Implementation rulings: the deployment service was
NOT modified (dry-run is honored inside the adapter), plan targets resolve
through an explicit `PlanTargetResolver`, secret references are declared by the
composition, and the health verdict comes from the port.

### Gate fixes required along the way

- `scripts/capabilities.sh` sorted with the ambient locale, so `make docs`
  failed on a `zh_CN.UTF-8` machine for a change that never touched the
  capability set. Sorting is now `LC_ALL=C` (byte order); the regenerated
  catalog is byte-identical to the committed one, so this is a determinism fix,
  not a content change.
- Six new accessor names classified in
  `openspec/governance/.reuse-classified-baseline` for the reuse ratchet.
- The disk hit 100% mid-run and the linker died with bus errors; clearing the
  17G incremental build cache restored the run. Not a code change.

### Evidence boundary (unchanged)

Rollback durability is per-process until `application-delivery-sqlite-state`
lands: a rollback to a release converged by an earlier process returns a typed
"never converged by this provider" error and mutates nothing.
