# compliance Specification

## Purpose
TBD - created by archiving change 2026-08-14-add-compliance-tooling. Update Purpose after archive.
## Requirements
### Requirement: CIS Hardening Wizard

`POST /admin/compliance/harden` SHALL apply a selected CIS/security
profile. Each applied rule SHALL store its pre-image so the change is
reversible, and the run SHALL produce a report of applied/failed/
skipped rules. Every change SHALL be audited.

#### Scenario: Harden applies and reports

- **WHEN** an Admin posts `/admin/compliance/harden` with a profile
- **THEN** a `HardeningRun` records each rule's pre-image and
        post-image, a report is returned, and an audit
        `HardeningApplied{profile}` is recorded.

#### Scenario: Hardening is reversible

- **WHEN** a previously applied rule is rolled back
- **THEN** the host state returns to the stored pre-image and an audit
        `HardeningReverted{rule}` is recorded.

### Requirement: Audit Retention Policy

`GET`/`PUT /admin/compliance/audit-retention` SHALL read and configure
an audit-log retention policy with a `ttl_days` (records older than
this are purged) and an optional scheduled export. The change SHALL be
audited.

#### Scenario: Set retention TTL

- **WHEN** an Admin sets `ttl_days=365` with export enabled
- **THEN** the `AuditRetentionPolicy` is persisted and an audit
        `AuditRetentionChanged` is recorded.

#### Scenario: TTL purge

- **WHEN** the retention job runs after the policy is set
- **THEN** only audit records older than `ttl_days` are purged and a
        `AuditPurged{count}` event is recorded.

### Requirement: GDPR Data Export

`GET /admin/compliance/gdpr-export/{user_id}` SHALL export all of a
user's PII across sites, mail, and databases. The export SHALL redact
secrets (database passwords, API tokens, private keys) and SHALL be
audited.

#### Scenario: Export redacts secrets

- **WHEN** an Admin exports user `u1`'s data
- **THEN** the export contains PII from sites/mail/db but every
        database password, API token, and private key is replaced with
        a redaction marker, and an audit
        `GdprExportRequested{user_id}` is recorded.

#### Scenario: Export scope covers all sources

- **WHEN** user `u1` has sites, mailboxes, and databases
- **THEN** the export aggregates PII from all three sources and omits
        no configured PII category.

