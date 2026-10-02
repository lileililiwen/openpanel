# Proposal: Production docker-backed delivery and deployment adapters

## Why

`BuildAdapter` / `RuntimeAdapter` (`openpanel-domain/src/
application_delivery/logic.rs:249,266`) and `DeploymentAdapter`
(`deployment_adapters/logic.rs:135`) are the ports OpenPanel's delivery and
deployment services consume. Only test fakes implement them; the trait docs
promise "OCI/Compose, generic systemd" adapters that no production code
provides. Consequently `forge-publish-provider-stdio` (authored 2026-09-30)
must compose fail-closed `Unavailable` markers, and Forge's `forge.publish`
cannot actually publish. Meanwhile the building blocks exist: the `docker`
module ships `BollardDockerAdapter` + `DockerModule` +
`SqliteDockerRepository`, and `container_registry`/`container_runtime`
provide registry operations. This change implements the three ports on top
of those primitives and replaces the Unavailable seam in the provider
composition.

## What Changes

- `DockerBuildAdapter` (`oci-docker-build-v1`): resolves an
  `ApplicationSource` (git ref or OCI digest) into an immutable
  `ReleaseDigest` by cloning/building with the existing docker service and
  pushing to the configured registry.
- `DockerRuntimeAdapter` (`oci-docker-runtime-v1`): `deploy`/`rollback`
  pull the pinned digest and converge the environment container via the
  docker service.
- `DockerDeploymentAdapter` publishing an `AdapterManifest`; `execute`
  validates plan-vs-manifest, honors dry-run and the cached idempotency
  evidence per the trait contract.
- Provider composition switch: `forge-provider` handler builds real service
  graphs; `Unavailable*` markers deleted (no shadowing per repo rules).

## Package map (split assessment)

| Package | Single outcome | Owner | Boundary/contract | Depends on | Independent oracle |
|---|---|---|---|---|---|
| `application-delivery-docker-adapters` (this) | publish/build/deploy executes for the provider call lifetime | openpanel-app/cli | three trait impls + composition | `forge-publish-provider-stdio`; its Unavailable seam names this as replacement | adapter unit tests vs docker fakes + integration harness |
| `application-delivery-sqlite-state` (future) | delivery records survive provider process exit (cross-run rollback) | openpanel-app | repository swap behind existing service APIs | this change's integration evidence | persistence round-trip tests |
| Forge consumer (roadmap B5) | `forge publish` end-to-end | forge | 0.1.0 contract | both above | forge e2e |

Adapters (behavior) and delivery-state persistence (storage lifecycle)
have separate oracles and land separately; bundling them would stall the
publish path on schema work.

## Capabilities

- **Modified:** `application-delivery`, `deployment-adapters` (production
  adapter implementations; service semantics unchanged).

## Non-goals

- No SQLite migration of in-memory delivery state (package above), no
  compose/systemd adapter variants, no k8s, no changes to trait definitions
  or `ApplicationDeliveryService` orchestration semantics, no changes to
  `forge_publish_provider` facade/protocol/transport, no new web UI.
