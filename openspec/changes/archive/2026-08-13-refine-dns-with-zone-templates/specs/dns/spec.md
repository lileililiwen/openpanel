## ADDED Requirements

### Requirement: Default Zone Templates

The dns context SHALL ship at least three compiled-in `ZoneTemplate` values: `strict` (default for production), `relaxed`, and `parked`. Every template MUST be a signed JSON manifest whose signature is verified on load; an invalid signature MUST cause startup to abort with exit code 78. Templates SHALL be versioned and recorded on every `Zone` along with the template instance created at `enable`.

#### Scenario: A template manifest fails signature verification

- **WHEN** the panel starts with a tampered `templates.json` whose signature does not match the master key
- **THEN** startup aborts with `EX_CONFIG` and the human-readable error names the offending template ID.

#### Scenario: A new template version is published

- **WHEN** a developer adds a new template and bumps its version
- **THEN** existing zones are unaffected; new zones opt-in via `template_version` on `Zone::enable`.

### Requirement: Template Application Lifecycle

The system SHALL apply the configured zone template atomically when `Zone::enable` is called. Each rendered record SHALL be attempted in order; failures SHALL set the record to `Pending` with a redacted diagnostic and SHALL NOT prevent the zone from becoming `Active`. The full template apply, including status flip, SHALL be one transaction.

#### Scenario: One template record fails at the provider

- **WHEN** template rendering produces 7 records and the SPF record fails validation
- **THEN** 6 records are persisted as `Active`, SPF is `Pending(reason="provider: invalid character at column N")`, the zone is `Active`, and an audit event `ZoneRecordPending` is recorded for the SPF record.

#### Scenario: Atomic rollback

- **WHEN** the database transaction fails mid-apply
- **THEN** no record rows are persisted and the zone remains in its prior status.

### Requirement: Template Re-Apply and Preview

Authorized callers SHALL preview or apply a template to an existing zone. The preview SHALL produce a typed plan listing every `op` (`create`, `replace`, `skip`, `delete`) against current records, every kind and value, and every conflict. The apply SHALL refuse a `replace_existing` request that lacks a typed `confirmed_at` UTC timestamp within ±60s of now and SHALL refuse without an authenticated Owner.

#### Scenario: Preview without mutation

- **WHEN** an Admin calls `POST /dns/zones/{id}/apply-template` with `mode=preview`
- **THEN** the response is a typed plan with no provider calls and no DB writes; no audit event is recorded.

#### Scenario: Apply with replace requires confirmation

- **WHEN** `mode=apply`, `replace_existing=true`, and the caller is not an Owner
- **THEN** the request is rejected with 403 and `apply_blocked_reason="requires_owner"`.

- **WHEN** `replace_existing=true` but `confirmed_at` is older than 60 seconds
- **THEN** the request is rejected with 400 and `apply_blocked_reason="stale_confirmation"`.

### Requirement: Template Override Per Owner

Owners SHALL be able to set a per-account `default_zone_template`. Owner-scoped preferences MUST shadow the global default and SHALL be auditable on every `enable`.

#### Scenario: An Owner sets the relaxed template

- **WHEN** an Owner calls `PUT /dns/preferences` with `{ default_zone_template: "relaxed" }`
- **THEN** subsequent zone enables for that Owner use `relaxed`; the value is recorded on each Zone as `template_instance_id`.

- **WHEN** the per-owner value is invalid (unknown template or signature mismatch)
- **THEN** the preference update is rejected and the global default remains in force.
