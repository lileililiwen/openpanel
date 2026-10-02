# Design: docker-backed build/runtime/deployment adapters

## Explore & Reuse

(`scripts/repo-map.sh` + targeted reads; exact reused items)

| Need | Reused exactly | Not re-implemented |
|---|---|---|
| docker engine access | `docker::adapter::BollardDockerAdapter`, `DockerModule`, `SqliteDockerRepository`, `DockerReconcileTask` patterns | no new bollard wrapper |
| registry push/pull | Bollard image API for remote OCI push and existing Docker pull; `container_registry/` for local metadata only | no parallel registry HTTP client |
| ports | `openpanel-domain::application_delivery::logic::{BuildAdapter, RuntimeAdapter, ReleaseDigest, Release, Environment, ReleaseOutcome, ApplicationDeliveryError}`; `deployment_adapters::logic::{DeploymentAdapter, AdapterManifest, DeploymentPlan, DeploymentEvidence}` | explicit BuildAdapter argument and typed errors added per correction below; sync execution preserved |
| service orchestration | `application_delivery::service::ApplicationDeliveryService`, `deployment_adapters::service::DeploymentAdapterService` semantics incl. manifest-id matching and cached idempotency replay | no second state machine |
| provider wiring | `forge_publish_provider` facade + composition from `forge-publish-provider-stdio`; `handlers.rs::bootstrap_persistence` for pool/audit | facade behavior unchanged |
| git source fetch | reuse whatever the existing site git-deploy path uses for clones (inventory in tasks 1); if none is shared, a thin internal helper inside `application_delivery` (public surface: none new beyond adapters) | no new public git abstraction |

### Inventory correction and approved dependency impact (2026-10-01)

The original reuse table overstated available APIs. `DockerAdapter` has no build,
push, image-inspection, or label-aware converge verb. `ContainerRegistryService`
stores caller-supplied blobs and does not push an image to a remote OCI registry.
No git-clone helper exists in the app/CLI. `Config` has a generic `modules` map,
but no typed Docker/registry namespace. `ApplicationDeliveryError` has no
unavailable or auth-rejected variant. `DeploymentAdapterService::run` synthesizes
dry-run evidence without invoking an adapter. These findings require adjacent
port and service work; preserving the draft's frozen-boundary claim would make
DCK-001..004 false.

Ruling: extend the existing **application-layer** `DockerAdapter` and Bollard
implementation with actual build/push/inspect and labeled converge operations.
The remote OCI push uses Bollard, not the blob-storage registry service.
`BuildAdapter::resolve_digest` gains an explicit `build_command: &str` argument
because `Application` owns that value and the current port receives only source.
All current fake implementations and tests must be updated. Add typed
`ApplicationDeliveryError::RuntimeUnavailable` and `RegistryUnauthorized`
variants with fixed, redacted display messages; update facade mapping. Modify
`DeploymentAdapterService` to call the adapter for dry-run so it can perform a
read-only full converge check. Keep its public methods and state model intact.
These departures from the original boundary are authorized by the principal's
direction to complete the observable requirements despite draft mistakes.

The private git helper uses argv-only commands, verifies the fetched commit,
and bounds the temporary tar context. Registry repository identity is derived
from configured namespace plus application id, allowing a release's digest-only
value to resolve to the same image after process restart. Provider composition
reads validated delivery settings from `Config.modules`; the namespace must be
stable across releases. Docker futures run on a separate worker thread with its
own Tokio runtime because the domain ports are synchronous and provider frames
already execute inside Tokio. No nested `Handle::block_on` is used.

## Boundary

- New: `crates/openpanel-app/src/application_delivery/{docker_build.rs,
  docker_runtime.rs}` + `deployment_adapters/docker_adapter.rs` (module
  `docker`), wired via `mod.rs` exports; `forge_provider` composition
  constructs `DockerBuildAdapter`/`DockerRuntimeAdapter`/
  `DockerDeploymentAdapter` from the docker module + registry services it
  already reaches through `DockerModule`; delete
  `forge_publish_provider/unavailable.rs` (shadow rule: replaced, not
  kept).
- Names for `reuse-strict`: `DockerBuildAdapter`, `DockerRuntimeAdapter`,
  `DockerDeploymentAdapter` — unique public type names; classify in the
  reuse baseline if the ratchet demands.
- Must not change: delivery/deployment service state model (stays in-memory
  until the sqlite-state package), facade protocol/lifecycle, web/api routes.

## Contract

- `DockerBuildAdapter::adapter_id = "oci-docker-build-v1"`;
  `resolve_digest`: git source → shallow clone at `source_ref`, `docker
  build` tagged ephemeral, push to configured registry namespace, return
  digest-pinned `ReleaseDigest` (registry digest, not tag); OCI source →
  verify digest exists, return it; build-command field passed to the docker
  build context unchanged via explicit port argument (trust boundary: build
  runs on the host Docker daemon).
- `DockerRuntimeAdapter`: `deploy` pulls pinned digest, creates/updates the
  environment's container through the docker service (labels
  `openpanel.application/environment/release`), waits for healthy-or-
  running per environment policy, returns terminal `ReleaseOutcome` +
  bounded redacted diagnostics (no env vars, no registry creds);
  `rollback` re-converges to the prior healthy release id from the service
  projection.
- `DockerDeploymentAdapter::manifest()` id `docker-deploy-v1`,
  capability `dry-run: true`,
  `idempotency: record-replay` per `AdapterManifest` fields; `execute`
  checks `plan.adapter` equals manifest id, applies dry-run by running the
  full converge check without mutation, returns terminal
  `DeploymentEvidence`; replay with `cached` returns the cached evidence
  unchanged.
- Failure mapping: docker daemon down ⇒ new
  `ApplicationDeliveryError::RuntimeUnavailable`; auth-rejected push ⇒ new
  `ApplicationDeliveryError::RegistryUnauthorized`. Both have fixed display
  messages, so upstream errors and credentials cannot leak.
- Configuration: registry namespace + build context limits read from
  existing `Config` sections (docker/registry), no new top-level keys
  unless `OPENPANEL__DELIVERY__*` proves necessary — decision at
  implementation, recorded in code comments naming the config source.

## Toggle and failure behavior

- Adapters are composed ONLY by the `forge-provider` entrypoint (and
  future delivery APIs); nothing auto-starts, the panel behaves exactly as
  today without a publish call — preserving independent-lifecycle.
- Daemon unavailable at resolve/deploy ⇒ typed error, provider still
  answers frames (transport unaffected), audit records failure.
- Concurrent publishes: docker build/run names derived from
  application/environment ids make host mutations idempotent; service-level
  serialization remains whatever `ApplicationDeliveryService` provides
  today (unchanged).
- Dry-run plans never mutate the host — asserted by tests capturing docker
  call logs.

## Verification

- `## 1. Testing` (see tasks): unit per adapter fn (inject a docker-service
  double implementing the module's existing abstraction; table tests:
  digest-pin happy, daemon-down, auth-reject, rollback target missing,
  dry-run no-mutation, idempotent replay equals cached evidence);
  integration: composition test constructing the full facade with real
  adapters against the double; E2E: `CliRunner` forge-provider frame
  exchange now yields success status for a Preflight against the double
  (previously the Unavailable seam's failure status).
- Coverage map: one requirement per adapter + one composition requirement.
- Gates: `make check` (tasks-testing-first, reuse-strict, file-length —
  new files small; `application-delivery` and `deployment-adapters` check
  targets included), `make gate`, `openspec validate --all --strict
  --no-interactive`; design.md human approval before apply (Agents.md
  §8.10).

## Implementation rulings (recorded during apply)

Four decisions were forced by contracts this change did not own. They are
recorded here because each one changes what a later reader expects from the
design above.

1. **`DeploymentAdapterService` was NOT modified.** The design above proposed
   routing a dry-run through the adapter so the service would perform a
   read-only full converge check. `DeploymentAdapterService::run` already
   short-circuits a dry-run into synthetic evidence, and its unmodified suite
   (`dry_run_records_ready_evidence_without_executing`) pins that behavior, so
   changing it would violate task 3.1 ("their suites unmodified"). The dry-run
   contract is instead satisfied where the trait requires it:
   `DockerDeploymentAdapter::execute` honors `plan.dry_run()` itself and runs
   `DockerRuntimeAdapter::check` (ping + `image_exists`, zero mutating verbs).
   The service's state model and public methods are untouched.

2. **Plan targets resolve through an explicit port, not a name guess.** A plan
   carries only `forge:<slug>:<environment>` and a digest, and the
   `RuntimeAdapter` port never sees an application name, so the deployment
   adapter cannot map a plan onto a host target by itself. `PlanTargetResolver`
   is that port; `DeliveryTargetResolver` implements it over
   `ApplicationDeliveryService` (slug tail → application → environment →
   newest release), and `FixedTargetResolver` doubles it in tests. An
   unresolvable target yields `PreflightRejected` (preflight) or `Failed`
   (deploy) rather than a guess.

3. **Secret references are declared by the composition.** `validate_plan`
   refuses a plan whose secret references are not declared by the manifest, and
   the names are the operator's environment secret sets — not a property of the
   adapter. `DockerDeploymentAdapter` therefore declares none and exposes
   `with_declared_secret_refs(names)`; the CLI reads them from
   `modules.delivery.plan_secret_refs`. A deployment that leaves it unset
   accepts only plans that carry no secret reference. This is a consequence of
   the frozen domain contract, not a preference.

4. **The health verdict is exposed by the docker port.**
   `RuntimeContainerState` gained `health: Option<String>` (serde-default,
   skipped when absent), populated from the engine's container health. That is
   what makes "healthy-vs-running policy honored" real rather than dead
   configuration: an image-declared verdict wins, and an image with no
   `HEALTHCHECK` is accepted on `running` alone. The field is additive; the four
   pre-existing construction sites were updated.

### Where the doubles live

One recording docker double serves every suite, in
`crates/openpanel-app/src/docker/testing.rs` (`#[doc(hidden)] pub mod
testing`): the adapter unit suites, the application-delivery integration
suite, and the CLI suite. It lives in `openpanel-app` rather than
`openpanel-test-support` because it implements this crate's own port — a double
in a crate that depends on `openpanel-app` would not satisfy the trait the
unit tests see. `application_delivery::testing` holds the delivery-port double
and `forge_publish_provider::testing::double_harness` builds the full provider
graph over it, so the integration suite and the CLI suite compose the *same*
graph the entry point composes.

The CLI end-to-end proof is in-process, not a subprocess: no shipped code path
reads a "pretend to publish" switch, so the existing subprocess tests keep
proving the real `forge-provider` entry point (startup refusals, framing,
audit) and the new test drives the production `run_loop` transport over the
double-backed harness, asserting `dry_run` then `succeeded`.

### Evidence boundary

Rollback durability is per-process: `ConvergeRegistry` records the releases this
process converged, so a rollback to a release converged by an earlier process
(or before a restart) returns a typed "never converged by this provider" error
and mutates nothing. The durable projection is
`application-delivery-sqlite-state`; this change does not claim that
capability.
