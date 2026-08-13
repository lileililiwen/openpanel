# Add Docker container management

## Why

Modern Baota ships a Docker module. A web owner who wants to run a
Redis sidecar, a Ghost container, or a static-site builder in
isolation can do so with a single click. OpenPanel currently has no
container story at all. This change adds a `docker` bounded context
that talks to the host's Docker daemon through a vetted Rust client
(`bollard`), restricted to a fixed allowlist of actions, and runs
every container as a non-root user inside a per-site network
namespace. No `docker.sock` mount-from-the-app — the panel uses
`HTTP over a unix socket` with explicit capability declarations.

## What Changes

- New `docker` bounded context with a `Container` aggregate scoped
  per site (or per host for system services), a `ComposeStack`
  aggregate, and a `ContainerImageAllowlist` registry.
- A typed `DockerAdapter` (bollard) that only exposes the panel's
  intended verbs (`pull`, `create`, `start`, `stop`, `restart`,
  `logs`, `inspect`, `remove`, `exec`). The panel never shells out
  to `docker`.
- All images MUST come from a configurable allowlist (default: the
  same trust root as the software catalog). Pulling an image not on
  the allowlist is rejected at the adapter.
- Containers are run with the user-namespace mapping `panel:<uid>`
  and dropped capabilities: `cap_net_bind_service` only when
  binding a privileged port, `cap_sys_admin` never.
- Per-container resource limits (CPU shares, memory) and restart
  policy.
- REST, CLI, and `/docker` web surface.

## Capabilities

### New Capabilities

- `docker`: container and compose-stack lifecycle over a vetted
  Docker adapter.

### Modified Capabilities

- `software-center`: image provenance reuses the same trust root
  for allowlisted images.

## Impact

- Domain: `Container`, `ComposeStack`, `ImageAllowlist` aggregates.
- App: `DockerService`, `DockerAdapter` (bollard) behind a port.
- API/CLI/web: `/api/v1/docker/{containers,stacks,images}`,
  `openpanel docker {pull,create,start,stop,restart,logs,rm,exec}`,
  `/docker` page.
- Dependency: `bollard` (async Docker Engine API client, MIT).
- The Docker daemon must be reachable at `unix:///var/run/docker.sock`
  or a configured TCP endpoint. The panel refuses to start the
  module if the socket is not present.
