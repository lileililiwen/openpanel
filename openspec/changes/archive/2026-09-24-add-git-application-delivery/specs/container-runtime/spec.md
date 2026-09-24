# container-runtime Specification

## Purpose

Per-user / per-site container runtime quotas and registry
credentials. The application-delivery change adds a typed
application / environment / release aggregate above the
existing container runtime primitives; the new aggregate
reuses the existing quota gate and registry credential
encryption.

## Requirements

## ADDED Requirements

### Requirement: Application Delivery Reuses Container Quotas

The application-delivery service MUST consult the existing
`container_runtime` quota check before a build is started. The
check is a per-owner enforcement; the new aggregate MUST NOT
introduce a parallel quota subsystem.

#### Scenario: Quota exceeded blocks the build

- **WHEN** an owner requests a new release and the quota gate
  refuses
- **THEN** the build is not started and an audit event records
  the refusal without the quota value.

### Requirement: Application Delivery Reuses Registry Credential Encryption

When an application delivery build resolves an OCI image, the
registry credentials MUST be resolved through the existing
`container_runtime` encrypted credential store. The application
delivery service MUST NOT store credentials in its own
repository.

#### Scenario: Credentials round-trip through the existing store

- **WHEN** an OCI source adapter requests a credential by name
- **THEN** the value is returned by the container-runtime
  credential port and the application delivery audit log does
  not include the credential value.
