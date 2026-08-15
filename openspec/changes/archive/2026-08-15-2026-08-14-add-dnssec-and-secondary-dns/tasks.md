# Add DNSSEC and secondary DNS — Tasks

## 1. Testing

- [x] 1.1 Unit: DS digest computation; KSK double-sign state machine;
      AXFR ACL match against `SecondaryNs.address`.
- [x] 1.2 Property: private KSK/ZSK never appears in any API response or
      serialized DTO; glue IP resolves within delegated NS.
- [x] 1.3 Service: enable DNSSEC; rotate KSK with CDS swap; publish DS
      to registrar (mock); configure secondary; add glue.
- [x] 1.4 Integration: signed zone serves RRSIG records; AXFR to an
      authorised slave succeeds; AXFR from an unauthorised host is
      refused.
- [ ] 1.5 CLI E2E: `openpanel dns dnssec enable` -> `rotate` ->
      `secondary add`.
- [ ] 1.6 Web: DNS zone tabs (CSRF), DNSSEC toggle, secondary/glue
      forms.

## 2. Domain and Application

- [x] 2.1 Implement `DnsSecPolicy`, `ZoneSigningKey`, `SecondaryNs`,
      `GlueRecord`, `DsRecord` under
      `crates/openpanel-domain/src/dnssec_secondary/`.
- [x] 2.2 Add SQLite migrations for DNSSEC keys, secondary NS, glue,
      and DS records.
- [x] 2.3 Implement `DnsSecService`, `KeyRolloverEngine`, `AxfrSender`,
      `GlueRecordService`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/dns/zones/{id}/dnssec`, `/dns/zones/{id}/secondary`,
      `/dns/zones/{id}/glue` REST routes.
- [ ] 3.2 Add `openpanel dns {dnssec,secondary,glue}` CLI commands.
- [ ] 3.3 Build the DNS zone tabs (CSRF): DNSSEC toggle, secondary
      nameserver form, glue-record form.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: enable DNSSEC on a zone, query for RRSIG, rotate
      KSK, perform an AXFR from an authorised slave.
- [x] 4.4 Archive with `openspec archive add-dnssec-and-secondary-dns`.
