# Proposal: Add operator security control plane

## Why

The dashboard can show selected security attention items, and OpenPanel has
firewall, malware, WAF, service-health, audit, and compliance capabilities, but
there is no unified remediation workflow. Mature panels make findings
prioritized, explainable, actionable, suppressible with expiry, and verifiable
after repair.

## What Changes

- Aggregate findings from existing security/compliance/scanner services.
- Add severity, evidence, affected resource, remediation, and verification state.
- Add safe one-click remediation with preview, authorization, idempotency, and
  rollback where supported.
- Add ignore/snooze policy with expiry and audit trail.
- Integrate summary, queue, notifications, API, CLI, and owner/admin web views.

## Capabilities

### New Capabilities

- `operator-security-control-plane`

### Modified Capabilities

- `operations-dashboard`
- `audit-activity`
- `host-security`
- `compliance`

## Non-goals

- No arbitrary shell execution.
- No automatic remediation for irreversible actions.
- No replacement of specialized WAF or malware engines.

## Dependencies

Depends on quality maturity and capability/navigation metadata. It reuses all
existing security services.
