# deployment-adapters Specification

## ADDED Requirements

### Requirement: A docker deployment adapter SHALL honor manifest, dry-run, and replay rules (DCK-003)

`DockerDeploymentAdapter` MUST publish an `AdapterManifest` with id
`docker-deploy-v1` and dry-run capability, refuse plans whose `adapter`
field does not match its manifest id, perform dry-runs as full converge
checks without host mutation, and return cached `DeploymentEvidence`
unchanged on idempotent replay.

#### Scenario: Foreign plan refused

- **WHEN** `execute` receives a plan naming a different adapter id
- **THEN** the call is refused with a manifest-mismatch error and the
  host is untouched

#### Scenario: Dry-run mutates nothing

- **WHEN** a matching plan carries the dry-run flag
- **THEN** the returned evidence is terminal and the docker double records
  zero mutations

#### Scenario: Replay returns cached evidence

- **WHEN** `execute` is called with `cached` evidence for the same plan
- **THEN** that evidence is returned verbatim without new host operations
