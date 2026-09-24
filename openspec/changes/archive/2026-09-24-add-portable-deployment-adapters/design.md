# Design: Add portable deployment adapters

## Approach

Model deployment as a provider-neutral action plan. The panel or CLI consumes
an adapter manifest that declares target capabilities and returns structured
prepare/deploy/verify/rollback evidence. Adapters own transport and host
policy; OpenPanel owns release identity, desired configuration, health checks,
and audit-safe lifecycle state.

## Explore & Reuse

- Reuse `openpanel-agent` mTLS registration and existing fleet health models.
- Reuse release provenance, installer rollback, monitoring notifications,
  audit events, and current command safety classification.
- Reuse `jenkins-local` only as a black-box conformance adapter; do not import
  its paths or scripts into product crates.

## Boundaries and safety

Every mutating action requires an explicit target, authorized caller, idempotent
operation key, and preflight result. Secrets are referenced by identifier and
never returned in plans or logs. A failed verification leaves the target in a
reported state and attempts rollback only when the adapter declares rollback
support.

## Verification

Test a fake adapter, a local OCI adapter, a generic Linux service adapter, and
the Mac/Jenkins fixture. Verify dry-run parity, retries, idempotency, audit
redaction, health failure, and rollback evidence.

## Approval

- **Status**: APPROVED as-is by the human principal on 2026-09-24 ("auto
  approve, no ask" directive for P3 of the portable production maturity
  queue).
- **Scope**: full apply end-to-end — implement the 4 ADDED requirements in
  the new `deployment-adapters` spec, modify `release-deployment-governance`,
  `monitoring-fleet-operations`, `terminal-host-fleet`, and `quality` per the
  proposal's "Modified Capabilities" list, and ship the Mac/Jenkins
  conformance fixture as a test-only artifact in `tests/integration/` (no
  product code may reference Mac, Jenkins, or the developer's workstation).
- **Reviewer note**: the principal's earlier correction (the
  `ConformanceAdapter` must NOT live in the product crate as a published
  module) is folded into the design — the Mac/Jenkins fixture is a test
  fixture only. The conformance path is required to pass locally but is
  NOT a required CI environment.
