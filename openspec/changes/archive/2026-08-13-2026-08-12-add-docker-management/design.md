# Add Docker container management — Design

## Domain model

```
Container
  { id, name (unique per host), image_ref, image_digest,
    site_id? (or null for system),
    env (encrypted_secrets subset),
    port_bindings[], resource_limits { cpu_shares, mem_mb,
                                       pids_max },
    restart_policy, user_namespace, dropped_caps[],
    status, created_at, started_at?, stopped_at? }

ComposeStack
  { id, name (unique per host), compose_yaml_canonical,
    signature_b64?, env (encrypted), site_id?,
    status, last_applied_at }

ImageAllowlist
  { ref_pattern (e.g. "library/redis:*", "ghcr.io/openpanel/*"),
    trust_root: software-center, allow_pull: true,
    pin_digest_required: bool }
```

## Adapter contract

The `DockerAdapter` port exposes only the verbs the panel needs:

```
async fn ping() -> Result<DaemonInfo, AdapterError>
async fn pull(ref: &ImageRef) -> Result<Digest, AdapterError>
async fn create(spec: ContainerSpec) -> Result<ContainerId, AdapterError>
async fn start(id) / stop(id) / restart(id) / remove(id, force)
async fn logs(id, since, until, tail) -> AsyncStream<LogLine>
async fn inspect(id) -> Result<ContainerState, AdapterError>
async fn exec(id, cmd[], user) -> Result<ExecResult, AdapterError>
async fn apply_stack(stack: ComposeStack) -> ApplyReport
```

- `pull` enforces the allowlist before issuing a network request.
- `create` rejects any spec that requests a host bind-mount outside
  `/var/lib/openpanel/containers/<id>/` or a port that is not in
  the site's allowed port range.
- `exec` runs as a non-root user unless the request explicitly
  carries `allow_root: true`, which is Owner-only and audited.

## Image allowlist

- The trust root is shared with the software catalog. A new image
  pattern can be added by signing the same way catalog manifests
  are signed.
- `pin_digest_required: true` for any pattern that runs in
  production (containers that bind a port); the panel refuses to
  start an unpinned production container.
- Updates to the allowlist are audited.

## Networking

- Per-site container joins a per-site `openpanel-<site_id>` bridge
  network created on first use.
- Containers can reach the site's database, but cannot reach the
  panel's control-plane port.
- Inbound ports: only those in the site's allowed port range
  (configurable; default 8080-8999) are exposed.

## Capabilities

- Default: `default` (no caps), `cap_net_bind_service` only when
  binding < 1024, `cap_chown` only when running as the panel user.
- `cap_sys_admin`, `cap_sys_ptrace`, `cap_sys_module`,
  `cap_net_admin`, `cap_net_raw`, `cap_dac_override` are forbidden.
- Seccomp profile: `runtime/default`. Custom seccomp requires
  Owner role and is audited.

## Resource limits

- `cpu_shares`: integer weight, default 1024.
- `mem_mb`: hard ceiling, default 512.
- `pids_max`: default 256.
- A container that exceeds memory is OOM-killed and the panel
  records a redacted audit row.

## Restart policy

- `no`, `on-failure[:max_retries]`, `always`, `unless-stopped`.
- Backoff is the Docker default; the panel exposes the policy as
  a string and never overrides it.

## Endpoints

```
GET    /api/v1/docker/images/allowlist
POST   /api/v1/docker/images/allowlist       Owner only
GET    /api/v1/docker/containers
POST   /api/v1/docker/containers
GET    /api/v1/docker/containers/{id}
POST   /api/v1/docker/containers/{id}/start
POST   /api/v1/docker/containers/{id}/stop
POST   /api/v1/docker/containers/{id}/restart
DELETE /api/v1/docker/containers/{id}
GET    /api/v1/docker/containers/{id}/logs
POST   /api/v1/docker/containers/{id}/exec
GET    /api/v1/docker/stacks
POST   /api/v1/docker/stacks
POST   /api/v1/docker/stacks/{id}/apply
DELETE /api/v1/docker/stacks/{id}
```

## Tests

```
1.1  Unit: ContainerSpec validation (port ranges, env secret
     envelope, capability list, user namespace).
1.2  Property: every forbidden capability is rejected;
     every image outside the allowlist is rejected;
     every host bind-mount outside the container root is rejected.
1.3  Service tests with mock bollard client covering happy path,
     pull failure, create failure, exec rejection, and
     OOM behaviour.
1.4  Integration: real bollard against a Docker-in-Docker or
     testcontainers runtime; pull from allowlist; create; start;
     logs; exec; remove.
1.5  CLI E2E: `openpanel docker {pull,create,start,stop,
     restart,logs,rm,exec}` with assertions on outputs.
1.6  Web: /docker with container list, create form (CSRF),
     log tail view, and a forbidden-capability warning banner.
```
