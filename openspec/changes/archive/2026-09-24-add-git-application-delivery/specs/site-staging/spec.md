# site-staging Specification

## Purpose

Per-site staging slots, sync policies, and atomic promote. The
application-delivery change adds a typed application / environment
/ release aggregate above the existing site staging primitives;
the new aggregate reuses the atomic swap for promotion and
rollback.

## Requirements

## ADDED Requirements

### Requirement: Application Delivery Promotion Is an Atomic Swap

When the application-delivery service promotes a release, the
swap MUST reuse the `site_staging` atomic-swap contract: the
prior release is retained until the new release is verified,
and a failed verification reverts to the prior state.

#### Scenario: Promotion reverts on failed health check

- **WHEN** a promotion triggers the runtime adapter and the
  health check fails
- **THEN** the prior healthy release remains the served target
  and the audit event records the failed transition without
  credentials.

### Requirement: Application Delivery Rollback Reuses Atomic Swap

The application-delivery rollback path MUST route through the
`site_staging` atomic swap. The operator confirmation is
captured, the prior healthy release is the new served target,
and the audit event records the transition.

#### Scenario: Rollback swaps back to the prior release

- **WHEN** an operator confirms a rollback and the runtime
  adapter reports `RolledBack`
- **THEN** the served target is the prior healthy release and
  the audit event records the transition with the release
  identity.
