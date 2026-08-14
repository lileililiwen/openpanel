# Add Compliance Tooling — Tasks

## 1. Testing

- [x] 1.1 Unit: CIS rule apply stores pre-image and reverts; TTL purge
      selects only older rows; secret redaction replaces credentials.
- [x] 1.2 Property: the GDPR export output never contains a raw
      secret pattern (DB password, API token, private key).
- [x] 1.3 Service: run harden + produce report; set retention policy;
      export user PII with secrets redacted.
- [x] 1.4 Integration: harden applies then rolls back to pre-image;
      export omits secrets across sites/mail/db.
- [ ] 1.5 Web: Compliance panel (harden wizard, retention form, export
      download).

## 2. Domain and Application

- [x] 2.1 Implement `HardeningRun`, `AuditRetentionPolicy`,
      `GdprExport` under `crates/openpanel-domain/src/compliance/`.
- [x] 2.2 Add SQLite migration for `hardening_runs`,
      `audit_retention_policy`, `gdpr_exports`.
- [x] 2.3 Implement `HardeningWizard`, `AuditRetentionService`,
      `GdprExporter`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/admin/compliance/harden`,
      `/admin/compliance/audit-retention` (GET/PUT),
      `/admin/compliance/gdpr-export/{user_id}` (Admin-gated).
- [ ] 3.2 Build the Compliance panel (harden wizard, retention form,
      export button).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: run harden + report, confirm rollback to
      pre-image; export a user, confirm secrets are redacted.
- [x] 4.4 Archive with `openspec archive add-compliance-tooling`.
