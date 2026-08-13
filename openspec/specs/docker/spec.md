# docker Specification

## Purpose
TBD - created by archiving change 2026-08-12-add-docker-management. Update Purpose after archive.
## Requirements
### Requirement: Image Allowlist and Provenance

The system SHALL maintain an Owner-managed `ImageAllowlist` of image reference patterns. Pulling an image whose reference does not match an allowlist entry MUST be rejected before any network request. Allowlist patterns MAY require a pinned digest; production containers (those that bind a port) SHALL refuse to start if the pattern requires a digest and the resolved image has none.

#### Scenario: Pull allowed image

- **WHEN** an Owner pulls `library/redis:7.4.0@sha256:<digest>` and `library/redis:*` is on the allowlist with `pin_digest_required: true`
- **THEN** the adapter fetches the image by digest, returns the digest, and records a `DockerImagePulled` audit event.

#### Scenario: Pull disallowed image

- **WHEN** a User tries to pull `evil/backdoor:latest`
- **THEN** the panel returns 403 and the adapter never contacts the Docker daemon.

### Requirement: Container Lifecycle

Owners SHALL create, start, stop, restart, inspect, log, exec, and remove containers. Each container is scoped to a site or to the host (for system services). The adapter SHALL reject any spec that requests a host bind-mount outside `/var/lib/openpanel/containers/<id>/` or a port outside the site's allowed port range.

#### Scenario: Create with site port

- **WHEN** an Owner creates a container that binds host port 8080 and the site allows 8080-8999
- **THEN** the container is created and a redacted summary is returned.

#### Scenario: Bind host-mount outside root

- **WHEN** a container spec requests `/etc` as a host bind-mount
- **THEN** creation fails with 422 and the request is audited as `DockerSpecRejected`.

### Requirement: Least Privilege Capabilities

Containers SHALL start with `default` capability set. `cap_net_bind_service` is granted only when binding a port < 1024. `cap_sys_admin`, `cap_sys_ptrace`, `cap_sys_module`, `cap_net_admin`, `cap_net_raw`, and `cap_dac_override` are forbidden under any caller. Any spec that requests a forbidden capability is rejected before the Docker daemon is contacted.

#### Scenario: Forbidden cap rejected

- **WHEN** a User submits a spec that requests `cap_sys_admin`
- **THEN** the panel returns 422 and audits `DockerCapabilityDenied`.

### Requirement: Resource Limits and Restart Policy

Every container SHALL declare CPU shares (default 1024), memory ceiling (default 512 MiB), and a PID ceiling (default 256). Restart policy is one of `no`, `on-failure[:max]`, `always`, `unless-stopped`. A container that exceeds its memory ceiling is OOM-killed by the kernel and the panel records a redacted audit row.

#### Scenario: OOM kill

- **WHEN** a container exceeds its memory ceiling
- **THEN** the kernel kills the container, the panel records `DockerOomKilled`, and the next inspect call shows `oom_killed: true`.

### Requirement: Compose Stack

Owners SHALL apply Compose stacks signed by the same trust root as the image allowlist. The panel SHALL canonicalise the YAML, verify the signature, and reconcile the live container set against the desired state. The live set is never permitted to drift beyond the reconciliation window (default 60 s).

#### Scenario: Apply signed stack

- **WHEN** an Owner applies a Compose stack whose signature validates
- **THEN** the panel reconciles to the desired state and returns an `ApplyReport` with the diff.

#### Scenario: Stack signature invalid

- **WHEN** the signature does not validate or the YAML is not canonical
- **THEN** the panel returns 400 and the stack is not applied.

### Requirement: Container Networking

Each site SHALL have a dedicated `openpanel-<site_id>` bridge network. Containers on that network can reach the site's database. Containers SHALL NOT be able to reach the panel's control-plane port. The bridge network is created on first use and removed when the last container on it stops.

#### Scenario: Network isolation

- **WHEN** a container in `openpanel-site42` tries to reach the panel's control plane
- **THEN** the connection is refused at the bridge level and the panel audits `DockerEgressDenied`.

### Requirement: Docker Surfaces

REST, CLI, and `/docker` web surfaces SHALL support the lifecycle described above. Browser mutations MUST enforce CSRF. Container logs are bounded, tail-only, and NEVER include environment variable values, even if the application prints them. Exec results are bounded to a 64 KiB payload; large outputs are truncated with a redacted notice.

#### Scenario: Owner uses a supported lifecycle surface

- **WHEN** an Owner creates a container, reads its bounded logs, and removes it through REST, CLI, or the web surface
- **THEN** every surface applies the same allowlist, least-privilege, ownership, redaction, and output-bound rules, and a browser mutation with a missing or invalid CSRF token is rejected before the Docker daemon is contacted
