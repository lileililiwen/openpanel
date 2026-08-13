# Add Docker container management — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `ContainerSpec` validation: port ranges,
      env secret envelope, capability list, user namespace.
- [x] 1.2 Property tests: every forbidden capability is rejected;
      every image outside the allowlist is rejected; every host
      bind-mount outside the container root is rejected.
- [x] 1.3 Service tests with a mock bollard client covering
      happy path, pull failure, create failure, exec rejection,
      and OOM behaviour.
- [x] 1.4 Integration: real bollard against a testcontainers
      runtime; pull from allowlist; create; start; logs; exec;
      remove.
- [x] 1.5 CLI E2E: `openpanel docker {pull,create,start,stop,
      restart,logs,rm,exec}` with assertions on outputs.
- [x] 1.6 Web: `/docker` container list, create form (CSRF),
      log tail view, and the forbidden-capability warning banner.

## 2. Domain and Application

- [x] 2.1 Implement `Container`, `ComposeStack`, and
      `ImageAllowlist` aggregates in
      `crates/openpanel-domain/src/docker/`.
- [x] 2.2 Add SQLite migrations and `SqliteDockerRepository`.
- [x] 2.3 Implement `DockerService` (CRUD, logs, exec) and the
      `DockerAdapter` port with the bollard implementation.
- [x] 2.4 Add the per-site bridge-network provisioning as a
      background step and a typed `NetworkAdapter` port.

## 3. Adapters and UI

- [x] 3.1 Add REST routes under `/api/v1/docker/{containers,
      stacks,images}`.
- [x] 3.2 Add `openpanel docker` CLI subcommands.
- [x] 3.3 Add `/docker` web pages with the create form, log tail,
      and the forbidden-capability warning.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: pull `redis:7` from the allowlist, create a
      container with a memory cap, observe OOM when exceeded,
      capture logs through the panel.
- [x] 4.4 Archive with `openspec archive add-docker-management`.
