# Design: Operator security control plane

## Approach

Introduce a normalized finding projection at the app layer. Source scanners
publish typed findings; the control plane stores lifecycle state and invokes
existing service operations through explicit remediation adapters. Each action
has preview, authorization, idempotency, post-check, audit, and notification
steps. Unsupported automatic fixes remain manual with exact evidence.

## Explore & Reuse

- Reuse `SecurityService`, compliance rule executor, malware scanner, WAF,
  service health, audit, and notification services.
- Reuse dashboard attention models, `TaskState`, confirmation/modal surfaces,
  and existing role/CSRF extractors.
- Reuse existing action allowlists and rollback/preflight patterns.

## Boundaries

Finding aggregation does not duplicate scanner logic. Remediation adapters own
one existing mutation each; the control plane owns lifecycle and evidence.
The web/API/CLI surfaces never accept raw commands or expose secret details.

## Verification

Test finding normalization, deduplication, severity ordering, authorization,
preview/confirm, idempotency, post-check failure, expiry, notifications, and
redaction through unit and integration suites.

## Non-goals

- New scanner engines.
- Automatic fixes without a typed adapter.
- Cross-host remediation before fleet permissions exist.
