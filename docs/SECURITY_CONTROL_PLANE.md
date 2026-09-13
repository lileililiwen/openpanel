# Security Control-Plane Runbook

Covers `operator-security-control-plane`: normalized findings queue,
typed remediation with preview and post-check, suppression with
expiry, and secret-safe evidence.

## Queue

- Web (Owner/Admin): `/security/findings` lists open findings ordered
  by severity (critical first), then source, resource, and rule.
  `Refresh from live services` re-ingests firewall blocks and
  degraded services; duplicates merge evidence under one stable id.
- API: `GET /api/v1/security/findings` (queue),
  `POST /api/v1/security/findings` (ingest 1–200 findings),
  `GET /api/v1/security/findings/{id}` (detail),
  `GET /api/v1/security/findings/{id}/preview` (adapter preview).
- CLI: `openpanel security findings queue` (seeds from live services,
  then prints the prioritized queue), `... show --id <id>`,
  `... preview --id <id>`.
- The dashboard attention queue links to `/security/findings` while
  any finding is open. Suppressed and resolved findings leave the
  queue; expiry returns a finding to the queue with no state loss.

## Remediation

Every automatic remediation follows the same gate:

1. `preview` names the typed adapter (`firewall_review`,
   `malware_quarantine`, `waf_tighten`, `compliance_rollback`,
   `service_restart`), its steps, whether confirmation is required
   (always true), and whether rollback is supported.
2. `remediate` requires an idempotency key (repeat calls are safe)
   and explicit confirmation. Unauthorized callers get `forbidden`
   and the denial is audited without credentials.
3. The adapter executes exactly one existing mutation, then a
   post-check runs. Success resolves the finding; a failed post-check
   leaves the finding `failed` with the recovery copy below and an
   audit event. Findings with no automatic adapter stay `manual`
   with exact evidence — never a blind fix, never a raw command.

- API: `POST .../{id}/remediate`
  `{"idempotency_key": "...", "confirmed": true}` (409 without
  confirmation; 422 for manual findings).
- CLI: `openpanel security findings remediate --id <id>
  --idempotency-key <key> --confirm`.
- Web: the detail page renders the preview and a confirm form.

## Suppression (ignore/snooze)

Suppressions require a reason, actor, scope, and a future expiry
(max 720h). Active suppressions hide the finding; expiry reopens it
as `open`.

- API: `POST .../{id}/suppress`
  `{"reason": "...", "scope": "...", "expires_at": "<rfc3339>"}`.
- CLI: `openpanel security findings suppress --id <id> --reason
  <...> --scope <...> --expires-in-hours 24`.
- Web: the detail page has a suppression form with the same fields.
- Every suppression emits a `settings_changed` audit event with
  redacted metadata.

## Recovery copy (post-check failures)

- `firewall_review`: last-known-good rules are restored
  automatically. Review the firewall preview and re-apply from
  `/security` after fixing the candidate.
- `malware_quarantine`: quarantine is kept. Restore within 24h from
  the malware scanner page if the detection was a false positive,
  then re-scan.
- `waf_tighten`: the ruleset was not persisted. Re-run dry-run from
  the site WAF page (`/sites/{id}/waf`) and save again.
- `compliance_rollback`: re-apply the hardening pre-image from the
  compliance run report; rollback targets are listed per rule.
- `service_restart`: inspect the service page (`/services`) and its
  logs before retrying; readiness probing refused the restart.

## Evidence safety

Evidence is redacted at the domain boundary (`redact_text`):
passwords, tokens, private keys, PEM blocks, raw command payloads,
and credential-like metadata become `[REDACTED]`, while resource
names, addresses, and rule ids stay actionable. API projections, web
pages, CLI output, and audit metadata all carry the redacted form.
If a scanner emits a new secret field, add its key to the redaction
list and extend the redaction unit tests first.
