# Refine DNS with zone templates — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `ZoneTemplate` rendering per record kind
      and conflict resolution with existing records.
- [x] 1.2 Property tests: every enabled zone has at least one
      A/AAAA; every zone has an SPF record; DMARC is present
      when MX is present.
- [ ] 1.3 Service tests with mock provider covering: apply
      success, apply under provider failure (Pending state),
      retry, audit. (deferred)
- [ ] 1.4 Integration: enable zone → all template records
      appear; failure of one record still flips zone to Active.
      (deferred)
- [ ] 1.5 CLI E2E. (deferred)
- [ ] 1.6 Web: Re-apply Template button. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `ZoneTemplate`, `TemplateRecord`,
      `TemplateName`, `TemplateRecordPolicy` under
      `crates/openpanel-domain/src/dns/templates.rs`.
- [ ] 2.2 Add SQLite migration for `zone_template_records` rows.
      (deferred)
- [ ] 2.3 Wire `DnsService::enable` to call `TemplateApplier::apply`
      atomically before flipping `Zone.status = Active`. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add `POST /api/v1/dns/zones/{id}/apply-template`. (deferred)
- [ ] 3.2 Add CLI subcommand. (deferred)
- [ ] 3.3 Add the Re-apply Template button. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: enable a fresh zone, inspect that strict
      template records exist. (deferred)
- [x] 4.4 Archive with `openspec archive refine-dns-with-zone-templates`.
