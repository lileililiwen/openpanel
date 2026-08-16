# container-runtime Specification

## Purpose
TBD - created by archiving change 2026-08-13-add-container-runtime. Update Purpose after archive.
## Requirements
### Requirement: Per-User Container Quota

`ContainerQuota` declares `max_concurrent`, `max_total`,
`cpu_pct_max`, `memory_bytes_max`, and
`egress_bytes_per_month`. The docker management service MUST
read the quota before every container create and MUST refuse
to start a container that would exceed any axis. The quota
MUST be plan-aware: a plan MAY restrict the axes even tighter.

#### Scenario: Quota exceeded

- **WHEN** an Owner has `max_concurrent = 4` and tries to start a 5th container
- **THEN** the start fails with `ContainerRuntimeError::QuotaExceeded{axis="concurrent"}` and the audit `ContainerStartQuotaBlocked` is recorded.

#### Scenario: Plan tighter than quota

- **WHEN** a plan says `memory_bytes_max ≤ 1GiB` for a user and the user has set `memory_bytes_max = 4GiB`
- **THEN** effective memory is 1GiB; the panel starts containers under 1GiB; audit `ContainerQuotaPlanOverride` records the new effective value.

### Requirement: Registry Credential Lifecycle

`RegistryCredential{ registry, username, password_ciphertext }`
SHALL be persisted with the password encrypted under the master
key. Plaintext passwords SHALL be returned exactly once on
create. Each credential MAY be rotated; rotation SHALL replace
the ciphertext and re-issue the plaintext once.

#### Scenario: Pull from registry

- **WHEN** an Owner submits `pull { image_ref, registry_credential_id }`
- **THEN** the panel calls the registry with the decrypted
        credentials, fails with `RegistryAuthFailed{redacted_reason}`
        on 401, succeeds on 200, and rotates the in-memory
        credential; the audit `ContainerImagePulled` records
        the image ref and ref-count only.

#### Scenario: Plaintext never echoed

- **WHEN** the panel reads the credential back via the API
- **THEN** the response is `{ id, registry, username }` and
        never includes the password.

### Requirement: Container Metrics

`GET /containers/{id}/metrics` SHALL return CPU, memory,
network rx/tx, and exit counters for a window (default 1
hour). The metric series SHALL be sampled every 5 seconds
in-process and pruned at the 1-hour retention boundary.

#### Scenario: Live metrics

- **WHEN** an Owner runs `metrics` on a long-running container
- **THEN** the response is `{ cpu_pct, memory_bytes, net_rx, net_tx, exits }` covering the requested window.

### Requirement: Egress Accounting and Thresholds

The system SHALL account for outbound network bytes per
container per month; the egress is recorded through the
`NetworkEgressAccount` observer. Threshold crossings emit
`BandwidthThresholdCrossed` events consumed by the bandwidth
change; the container that crossed the threshold receives a
policy event of `Throttle` (1 Mbps cap) until the owner
raises the limit.

#### Scenario: Threshold cross

- **WHEN** a container crosses 80% of its `egress_bytes_per_month`
- **THEN** the egress consumer emits a threshold event and
        the container is throttled to 1 Mbps until reset.

#### Scenario: Owner raises egress

- **WHEN** an Owner PUTs a new limit
- **THEN** throttle is removed; the audit `ContainerEgressLimitRaised` is recorded.

### Requirement: Audit Hygiene

Every start, stop, pull, and quota change is audited. The
audit MUST NOT include the plaintext password, the image
manifest body, or the registry response body. Allowed fields
are limited to: `{owner_id, container_id, image_ref, ref_count,
redacted_reason}`.

#### Scenario: Pull audit

- **WHEN** an image is pulled
- **THEN** audit `ContainerImagePulled{owner_id, container_id, image_ref, ref_count}` is recorded; no other payload.

