# Design: forge-publish-provider

## Explore & Reuse

Reuse `openpanel_domain::deployment_adapters::DeploymentAdapter`,
`openpanel_app::application_delivery::ApplicationDeliveryService`,
`openpanel_domain::git_deployment` webhook verification, the existing plugin
manifest/runtime model, and canonical audit redaction. Do not create a second
release or rollback state machine.

## Boundary

Add the provider facade under `crates/openpanel-app/src/forge_provider/` and a
thin composition/stdio adapter under the existing outer adapter layer. The
domain remains I/O-free. The provider speaks Forge JSON request/response
messages over stdio or JSON-RPC and is not linked directly into Forge.

## Contract

Accept `forge-publish-provider/0.1.0` operations `capabilities`, `preflight`,
`publish`, `verify`, and `rollback`. Require project, full revision, operation
identity, and dry-run state. Map release digest and secret references into
existing OpenPanel typed values; never accept raw secret values.

Return terminal status, health, rollback status, redacted diagnostics, and
provider identity. Invalid requests fail before mutation. Duplicate operation
ids use existing idempotency behavior.

## Toggle and failure behavior

Forge controls whether this provider is enabled. OpenPanel startup must remain
successful when the provider is absent or disabled. A malformed request,
missing application/environment, failed health check, or unsupported action
returns a typed failure and audit event. Rollback uses the existing
application-delivery rollback policy.

## Verification

Add unit tests for request translation and redaction, conformance fixtures for
all contract operations, integration tests for duplicate operations and
rollback, and a process-level test proving provider start/stop does not affect
the main OpenPanel service.
