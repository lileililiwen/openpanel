# release-deployment-governance Specification

## Requirements

## ADDED Requirements

### Requirement: Adapter-Based Deployment Is Provider-Neutral

A deployment that publishes a release artifact or runs a release-time action
MUST route through a registered deployment adapter that declares its
capabilities, health method, and rollback support, instead of coupling the
release pipeline to a specific workstation, CI vendor, or orchestrator. The
adapter manifest and evidence record are owned by the `deployment-adapters`
capability; release-time tooling MUST consume that contract and MUST NOT
introduce a second one.

#### Scenario: Release targets a provider-neutral adapter

- **WHEN** a release workflow targets a workstation, container runtime, or
  remote orchestrator
- **THEN** the workflow selects the matching adapter by its declared
  `target_scheme` and `actions` and rejects the target when no adapter
  declares support for the requested action.
