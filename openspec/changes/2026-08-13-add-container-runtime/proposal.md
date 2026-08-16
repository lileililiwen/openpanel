# Add container runtime

## Why

`openspec/.../add-docker-management` is in flight for Docker
container management. It introduces basic lifecycle
(create/start/stop/delete). cPanel and Baota go further:
each user has a **per-user container quota**, images can be
pushed from a panel-managed registry, and CPU / memory caps
are enforced. This change extends the docker-management
context with quotas and registry auth. The two changes are
compatible (this change consumes the docker cap as a
dependency and adds the missing operational policies).

## What Changes

- New bounded context `container-runtime` carrying
  `ContainerQuota`, `RegistryCredential`, `ContainerMetrics`,
  `NetworkEgressAccount`.
- New endpoints: `GET/PUT /container-quota`,
  `POST /containers/{id}/metrics`,
  `POST /registry/credentials`,
  `POST /containers/{id}/pull`,
  `POST /containers/{id}/egress/limit`.
- Coupling: integrates with the in-flight
  `add-docker-management` cap and `add-resource-quotas`.

## Capabilities

### New Capabilities

- `container-runtime`: per-user container quota, registry auth,
  metrics, and network egress accounting.

## Impact

- Domain: `ContainerQuota`, `RegistryCredential`,
  `ContainerMetrics`, `NetworkEgressAccount`.
- App: `ContainerQuotaService`, `RegistryAdapter`,
  `EgressAccount`, metrics poller.
- API/CLI/web: `/container-quota`, `/containers/{id}/metrics`,
  `/registry/*`; CLI `openpanel container quota`,
  `openpanel container metrics`.
- Coupling: depends on `add-docker-management` for the
  container surface and on `resource-quotas` for the shared
  quota axis.
