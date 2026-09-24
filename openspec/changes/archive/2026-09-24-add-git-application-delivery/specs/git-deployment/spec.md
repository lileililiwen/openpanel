# git-deployment Specification

## Purpose

Per-site Git deployment. The application-delivery change adds a
typed application / environment / release aggregate above the
existing Git deployment primitives. The Git deployment path
remains the canonical per-site Git flow; the new aggregate is
provider-neutral and composes Git with OCI and Compose inputs.

## Requirements

## MODIFIED Requirements

### Requirement: Preview Limits and Expiry

The system SHALL enforce a per-repository concurrent-preview cap and a
TTL after which a background reaper destroys expired previews;
plan quotas SHALL apply to preview counts.

#### Scenario: Cap enforced

- **WHEN** more than `max_per_repo` live previews are requested for
        one repository
- **THEN** creation fails with `PreviewError::CapReached` unless an
          expired preview can be reclaimed first.

#### Scenario: TTL expiry

- **WHEN** a preview has been Ready longer than `ttl_hours`
- **THEN** the reaper destroys it and audits
          `PreviewDestroyed{reason: "expired"}`.

## ADDED Requirements

### Requirement: Application Delivery Reuses Git Webhook Verification

Application delivery deployments that target a Git source MUST
reuse the `git_deployment::verify_webhook` verifier. The
verification key, body, and presented signature flow through the
existing function unchanged; the application-delivery service
MUST NOT introduce a parallel HMAC verifier.

#### Scenario: Git webhook signature is verified through the existing helper

- **WHEN** a Git source adapter receives a webhook payload
- **THEN** verification is delegated to
  `openpanel_domain::git_deployment::verify_webhook` and the
  deployment proceeds only when the helper returns `Ok`.

#### Scenario: Bad signature aborts before any side effect

- **WHEN** the presented signature does not match the expected
  digest
- **THEN** no build or deploy is started and an audit event
  records the refusal without the raw signature.
