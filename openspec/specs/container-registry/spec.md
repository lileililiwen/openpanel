# container-registry Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-container-image-registry. Update Purpose after archive.
## Requirements
### Requirement: Hosted OCI Registry

The system SHALL host an image registry conforming to the OCI
Distribution spec so standard `docker push`/`pull` clients work against
it. Push and pull SHALL be authenticated via the panel identity, and
blobs SHALL be stored only under the owning namespace path.

#### Scenario: Authorised push

- **WHEN** an authenticated user pushes an image to a namespace they own
- **THEN** the manifest and blobs are stored under that namespace and a
        `RegistryImagePushed` audit entry is recorded.

#### Scenario: Cross-namespace push rejected

- **WHEN** a user pushes to a namespace they do not own
- **THEN** the push is rejected with `403` and no blobs are written.

### Requirement: Per-User Namespaces

The system SHALL provide per-user namespaces that isolate tenant image
storage and SHALL enforce a per-namespace quota. Exceeding the quota
SHALL reject the push rather than over-commit storage.

#### Scenario: Quota enforced

- **WHEN** a push would exceed the namespace quota
- **THEN** the push is rejected and `used_bytes` is unchanged.

#### Scenario: Namespace created

- **WHEN** an admin or owner creates a namespace for a user
- **THEN** an `ImageNamespace` row exists with the owner and quota and
        is isolated from other tenants.

### Requirement: Retention and Scan Hook

The system SHALL apply a configurable retention policy (count and/or age
based) to prune stored images, and SHALL invoke a vulnerability-scan
hook after each push when `scan_on_push` is enabled. Scan findings SHALL
be recorded as a `ScanResult` and SHALL NOT auto-delete images.

#### Scenario: Scan after push

- **WHEN** an image is pushed with `scan_on_push` enabled
- **THEN** a `ScanResult` is stored for the digest and the image remains
        present regardless of findings.

#### Scenario: Retention prune

- **WHEN** the retention policy exceeds its limit for a namespace
- **THEN** the oldest or over-count images are pruned and remaining
        images stay within the policy.

