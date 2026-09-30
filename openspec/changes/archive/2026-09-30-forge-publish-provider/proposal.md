# Proposal: forge-publish-provider

## Why

OpenPanel already owns application delivery, Git deployment, deployment
adapters, audit, idempotency, and rollback. It should be testable as an
optional Forge provider without moving those responsibilities into Forge or
turning OpenPanel into a narrow deployment utility.

## What Changes

- Add a thin Forge provider facade in the OpenPanel adapter layer.
- Translate `forge-publish-provider/0.1.0` requests into existing typed
  application-delivery and deployment-adapter operations.
- Return redacted terminal evidence, health, rollback, and capability data.
- Allow the provider process to be started, stopped, and tested independently.

## Capabilities

- **New:** Forge publish provider facade.
- **Modified:** application-delivery composition exposes provider-safe entry
  points; existing domain contracts remain unchanged.

## Non-goals

- Replacing OpenPanel’s existing API, CLI, or plugin marketplace.
- Moving Forge orchestration into OpenPanel.
- Logging secrets, source contents, or provider transport commands.
- Making the provider mandatory for OpenPanel startup.
