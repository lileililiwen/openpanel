# operator-security-control-plane Specification

## Purpose

The dashboard can show selected security attention items, and OpenPanel has firewall, malware, WAF, service-health, audit, and compliance capabilities, but there is no unified remediation workflow. Mature panels make findings prioritized, explainable, actionable, suppressible with expiry, and verifiable after repair.
## Requirements
### Requirement: Findings Are Normalized and Prioritized

The control plane MUST normalize findings with stable ID, source, severity,
resource scope, evidence, remediation mode, state, and timestamps, then order
them deterministically.

#### Scenario: Duplicate scanner result

- **WHEN** two sources report the same resource and rule
- **THEN** the operator sees one finding with merged evidence and source links.

### Requirement: Remediation Is Previewed and Verified

Automatic remediation MUST be typed, authorized, idempotent, auditable, and
followed by a post-check; unsupported fixes MUST be manual.

#### Scenario: Remediation post-check fails

- **WHEN** an action reports success but the post-check remains unhealthy
- **THEN** the finding remains open with a failure state and recovery guidance.

### Requirement: Suppression Expires

Ignore/snooze operations MUST require a reason, actor, scope, and expiry, and
MUST reopen or re-evaluate the finding when the expiry passes.

#### Scenario: Suppression expires

- **WHEN** a finding reaches its suppression expiry
- **THEN** it returns to the prioritized queue and emits no silent state loss.

### Requirement: Operators See Safe Evidence

Web/API/CLI and audit views MUST show enough evidence to act while redacting
credentials, private keys, raw command payloads, and sensitive file contents.

#### Scenario: Finding contains secret metadata

- **WHEN** a scanner includes credential-like metadata
- **THEN** the projection and audit event omit or redact it.

