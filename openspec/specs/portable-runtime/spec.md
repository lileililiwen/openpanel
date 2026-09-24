# portable-runtime Specification

## Purpose

Portable runtime packaging defines how OpenPanel is installed, configured,
health-checked, upgraded, and recovered on supported hosts and OCI runtimes.

OpenPanel ships two official adapters for one runtime contract: a native
Linux service and an OCI image. Both consume the same configuration
schema, declare the same persistent data location, expose the same
health/readiness surface, and honour the same graceful-shutdown contract.
A deployment adapter (Compose, systemd, Podman, Kubernetes, Mac
deployment repository) supervises the adapter; the adapter never reads
developer-checkout state, never infers credentials from local shell
state, and never silently falls back to an unsupported platform.

## Requirements

### Requirement: Provider-Neutral Runtime Contract

OpenPanel MUST publish one runtime contract covering binary identity,
configuration, persistent data, health/readiness, graceful shutdown,
migration ceiling, and rollback behaviour for every official adapter.

The contract is the union of:
- the supported (OS, architecture) matrix published in the release's
  `target-manifest.txt`,
- the environment variables and configuration keys consumed by both
  adapters (notably `OPENPANEL_DATA_DIR`, `OPENPANEL_CONFIG`, and the
  `RUST_LOG` family),
- the persistent data location the adapter MUST mount (default
  `/var/lib/openpanel` on native, `OPENPANEL_DATA_DIR` on OCI),
- the health endpoint the orchestrator probes (`/health` for liveness,
  the binary's `healthcheck` subcommand as a probe-friendly
  alternative),
- the signal that requests graceful shutdown (`SIGTERM`),
- the schema ceiling published by the binary
  (`OPENPANEL_MAX_SCHEMA_VERSION`),
- the rollback target the upgrade path must retain.

#### Scenario: Equivalent adapters

- **WHEN** the same release is started through the native Linux
  adapter (`packages/installer/install.sh` + `entrypoint.sh`) and the
  OCI adapter (`Dockerfile` + the same `entrypoint.sh`)
- **THEN** both expose the same `OPENPANEL_DATA_DIR`, the same
  `OPENPANEL_CONFIG` shape, the same `/health` response, and the
  same behaviour on `SIGTERM`.

### Requirement: Supported Target Manifest

The release MUST publish a `target-manifest.txt` at the repository root
of the installer package. The manifest lists every supported
(operating-system, architecture) pair, one per line, in the form
`<id> <arch>`, where `<id>` is the value of `ID` in `/etc/os-release`
on the supported distribution and `<arch>` is the Rust
`std::env::consts::ARCH` value.

The native installer MUST consult the manifest before any filesystem
mutation. A host whose (OS, architecture) pair is absent from the
manifest MUST cause `install` to exit `78` (EX_CONFIG) with an
actionable compatibility error and MUST NOT create the service
account, the data directory, or any other partial state.

The manifest is the single source of truth for "what is supported".
Both `install.sh` and any future deployment adapter MUST derive their
support decision from the same file; a per-script hard-coded list is
not authoritative.

#### Scenario: Unsupported target

- **WHEN** an operator runs the native installer on an OS whose `ID`
  is not in `target-manifest.txt`
- **THEN** installation exits with code `78` and prints
  `unsupported distribution: <id>` to stderr, and the data directory
  and service user are NOT created.

### Requirement: Persistent Data and Secret Boundary

An adapter MUST provide an explicit persistent data location
(declared as a `VOLUME` on the OCI adapter, as `OPENPANEL_DATA_DIR`
on the native adapter) and MUST inject secrets and configuration
through environment variables or a mounted config file rather than
through image layers, build arguments, source-control paths, or
public release artifacts.

The Dockerfile MUST NOT contain any literal credential
(`password=`, `api_key=`, `token=`, `secret=`), MUST NOT `COPY` a
config file containing credentials, and MUST NOT `ARG`-declare any
secret value. A unit test scans the Dockerfile and the
`packages/installer/` tree for credential-shaped literals and fails
if any are present.

The OCI adapter MUST fail to start if the declared data volume is
not mounted; the native adapter MUST refuse to install if
`OPENPANEL_DATA_DIR` is not writable.

#### Scenario: Missing persistent data

- **WHEN** the OCI runtime starts the container without mounting the
  declared `OPENPANEL_DATA_DIR` volume
- **THEN** the entrypoint exits non-zero before binding the
  listening port and the diagnostic names the required mount path.

### Requirement: Safe Upgrade and Rollback

Every official adapter MUST preflight the database schema ceiling
against the binary's `OPENPANEL_MAX_SCHEMA_VERSION` before replacing
the installed binary. The adapter MUST retain a recoverable previous
binary (e.g. `openpanel.bak.<UTC>`) and MUST verify the new binary's
`/health` (or its `healthcheck` subcommand) after the swap. A failed
post-upgrade health check MUST restore the previous binary
automatically and exit non-zero with a diagnostic naming the failed
probe.

A schema newer than the binary's ceiling MUST be rejected with an
actionable diagnostic that names the applied schema version, the
ceiling, and the remediation (restore from a compatible backup).
The `rollback` subcommand MUST restore the most recent retained
backup without otherwise mutating state.

#### Scenario: Failed post-upgrade health

- **WHEN** the new release fails its post-upgrade health check
- **THEN** the installer restores the previous binary from the
  `.bak.<UTC>` slot, exits non-zero, and prints
  `post-upgrade smoke check failed; rolled back to <path>`.
