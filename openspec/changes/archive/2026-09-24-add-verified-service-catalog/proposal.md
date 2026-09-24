# Proposal: Add a verified ecosystem service catalog

## Why

1Panel and aaPanel reduce adoption friction through curated application and
service catalogs. OpenPanel has a software center and installer types, but a
real ecosystem needs signed, versioned, reviewable templates with upgrade,
backup, health, and removal semantics that do not turn the panel into an
unbounded arbitrary-script runner.

## What Changes

- Define a signed service manifest and trust/publisher model.
- Support versioned OCI/Compose service templates with declared resources,
  ports, storage, dependencies, health, backup, and upgrade behavior.
- Add install/upgrade/rollback/remove lifecycle and catalog review states.
- Make community contributions auditable and reproducible.

## BFS Impact Map

- Capabilities: `software-center`, `software-center-trust`, `container-runtime`,
  `backups`, `web-application-installer`, and `web-ui` are modified.
- Users: self-hosters, service publishers, reviewers, and operators.
- Contracts: manifest schema, signatures, publisher identity, compatibility,
  lifecycle jobs, and trust decisions.
- Integrations: OCI registries and object storage are optional adapters.
- Failure boundaries: invalid signature, incompatible target, resource
  conflict, failed install/upgrade, backup failure, and malicious template.
- Unaffected: arbitrary shell/plugin execution and private credentials.

## Capabilities

### Modified Capabilities

- `software-center`
- `software-center-trust`
- `container-runtime`
- `backups`
- `web-application-installer`

## Non-goals

- No unreviewed marketplace execution.
- No requirement for one central commercial catalog.
- No promise that every third-party service is portable across every host.
- No catalog entry may bypass host policy, quotas, or authorization.
