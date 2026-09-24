# portable-runtime Specification

## Purpose

Portable runtime packaging defines how OpenPanel is installed, configured,
health-checked, upgraded, and recovered on supported hosts and OCI runtimes.

## ADDED Requirements

### Requirement: Provider-Neutral Runtime Contract

OpenPanel MUST publish one runtime contract covering binary identity,
configuration, persistent data, health/readiness, graceful shutdown, migration
ceiling, and rollback behavior for every official adapter.

#### Scenario: Equivalent adapters

- **WHEN** the same release is started through native Linux and OCI adapters
- **THEN** both expose the same config validation, health status, data location
  semantics, and shutdown result.

### Requirement: Supported Target Manifest

The release MUST publish supported operating-system, architecture, and runtime
targets with an explicit unsupported result for targets outside the matrix.

#### Scenario: Unsupported target

- **WHEN** an operator runs the native installer on an unsupported OS
- **THEN** installation exits with a documented compatibility error and makes
  no partial service claim.

### Requirement: Persistent Data and Secret Boundary

An adapter MUST provide an explicit persistent data location and inject
secrets/configuration without copying them into source control, image layers,
logs, or public release artifacts.

#### Scenario: Missing persistent data

- **WHEN** an OCI runtime cannot mount the declared data location
- **THEN** startup fails before serving traffic and reports the remediation.

### Requirement: Safe Upgrade and Rollback

Every official adapter MUST preflight configuration and schema compatibility,
retain a recoverable previous release, and verify health after upgrade before
declaring success.

#### Scenario: Failed post-upgrade health

- **WHEN** the new release fails its post-upgrade health check
- **THEN** the adapter restores the previous release and reports rollback
  evidence.
