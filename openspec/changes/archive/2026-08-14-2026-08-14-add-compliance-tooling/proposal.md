# Add Compliance Tooling

## Why

OpenPanel has **no CIS hardening wizard, GDPR data export, or
audit-retention policy**. Enterprise buyers expect documented security
hardening, the ability to export a user's personal data on request, and
a configurable audit-log retention policy. Without these, sales into
regulated environments stall. This change adds a `compliance` bounded
context.

## What Changes

- New bounded context `compliance` carrying the `HardeningRun`,
  `AuditRetentionPolicy`, and `GdprExport` aggregates with
  `HardeningWizard`, `AuditRetentionService`, `GdprExporter`.
- New endpoints: `POST /admin/compliance/harden`,
  `GET /admin/compliance/audit-retention`,
  `PUT /admin/compliance/audit-retention`,
  `GET /admin/compliance/gdpr-export/{user_id}`.
- A CIS/security hardening wizard (apply + report), an audit-log
  retention policy (TTL + export), and a GDPR data export per user
  covering all PII across sites/mail/db.

## Capabilities

### New Capabilities

- `compliance`: apply a reversible, recorded CIS hardening baseline
  with a report; configure audit-log retention (TTL + export); and
  export a user's PII across sites/mail/databases for GDPR requests.

## Impact

- Domain: `HardeningRun`, `AuditRetentionPolicy`, `GdprExport`.
- App: `HardeningWizard`, `AuditRetentionService`, `GdprExporter`.
- API/CLI/web: `/admin/compliance/*`, web Admin → Compliance panel.
- Security: `gdpr-export` redacts secrets (DB passwords, API tokens,
  private keys); hardening changes are reversible and recorded; all
  actions Admin-gated and audited.
- Coupling: reports feed the audit view from
  `refine-web-ui-with-audit-accessibility-theming`; depends on
  `host-security` posture and `identity` for user/PII scope.

## What Changes

- (Security note) `gdpr-export` MUST redact credentials and secrets;
  hardening MUST be reversible and recorded so a change can be rolled
  back and reviewed.
