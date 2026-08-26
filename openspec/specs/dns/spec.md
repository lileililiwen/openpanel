# dns Specification

## Purpose

The DNS bounded context covers provider accounts, zones, records,
and lifecycle. After this refinement, it also owns the
`ZoneTemplate`, `TemplateRecord`, and `TemplateName` value
objects that the template applier consumes at zone enable time.

## Requirements

### Requirement: Zone Template

The DNS bounded context SHALL model a `ZoneTemplate` carrying `name: TemplateName` (`Strict | Relaxed | Parked`) and `records: Vec<TemplateRecord>`. The constructor rejects empty template record lists.

#### Scenario: Empty template rejected

- **WHEN** `ZoneTemplate::new(TemplateName::Parked, vec![])` is called
- **THEN** the constructor returns `TemplateError::EmptyTemplate`.

### Requirement: Template Record

The DNS bounded context SHALL model a `TemplateRecord { kind, name, ttl, value, policy }`. The constructor rejects empty kind or name.

#### Scenario: Empty kind rejected

- **WHEN** `TemplateRecord::new("", "@", 60, "v=spf1", TemplateRecordPolicy::Required)` is called
- **THEN** the constructor returns `TemplateError::InvalidRecord`.

### Requirement: Built-in Templates

The DNS bounded context SHALL expose `ZoneTemplate::strict(domain)`, `ZoneTemplate::relaxed(domain)`, and `ZoneTemplate::parked()` as the three built-in templates. The strict template includes apex A/AAAA, MX, SPF, DMARC, and an optional CAA record. The relaxed template includes apex A and SPF. The parked template includes a single parking-page A record.

#### Scenario: Strict template has required records

- **WHEN** `ZoneTemplate::strict("example.com")` is built
- **THEN** `required_records().count() >= 3` and at least one SPF + one DMARC record is present.

#### Scenario: Lookup returns built-in templates

- **WHEN** `ZoneTemplate::lookup(TemplateName::Strict, ...)` is called
- **THEN** the returned template has `name = TemplateName::Strict`.

### Requirement: Behaviour Parity

The refinement SHALL introduce the new template types without changing the existing zone lifecycle. The follow-on `apply-template` change wires the applier into `DnsService::enable`.


#### Scenario: Existing behaviour unchanged

- **WHEN** the refined bounded context is exercised through its public API
- **THEN** behaviour outside the newly added surface is identical to the pre-refinement behaviour.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit `DnsTemplateApplied` and `DnsTemplatePreviewed` audit events. The bounded context as archived today owns the typed model and the in-memory template renderers.

### Requirement: Per-Zone DNSSEC Signing

The system SHALL let an authorised caller enable DNSSEC signing for a
zone with separate ZSK and KSK keys. The private portions of the keys
SHALL be held in key custody and SHALL NOT be returned by any API
response. The system SHALL publish DNSKEY, RRSIG, and CDS/CDNSKEY
records.

#### Scenario: Enable signing

- **WHEN** an Admin `PUT /dns/zones/{z1}/dnssec` with `enabled=true`
- **THEN** the zone is signed with ZSK+KSK, RRSIG/DNSKEY/CDS records are
        published, and the response contains no private key material.

#### Scenario: Private key never exposed

- **WHEN** any DNSSEC resource is serialized to an API response
- **THEN** only public key blobs (tag, algorithm, state) are present;
        private key material is absent.

### Requirement: KSK/ZSK Rollover

The system SHALL support key rollover using a double-signing state
machine. A KSK rollover SHALL publish a new KSK, emit the new CDS,
and only retire the old KSK after the parent DS is swapped.

#### Scenario: KSK rollover

- **WHEN** an Admin `POST /dns/zones/{z1}/dnssec/rotate` with
        `role=Ksk`
- **THEN** a new KSK enters `Published`, the new CDS is published, the
        old KSK is retired only after DS swap, and an audit
        `DnsSecRollover{role}` is recorded.

### Requirement: DS Publication to Registrar

The system SHALL compute the `DsRecord` from the active KSK and SHALL
offer publication to the registrar (coupling with `ssl` registrar
publishing flow).

#### Scenario: DS computed and published

- **WHEN** DNSSEC is enabled and the DS is published
- **THEN** the `DsRecord{digest}` is computed from the KSK and handed to
        the registrar publish path; the digest only is stored.

### Requirement: Secondary DNS / AXFR

The system SHALL allow a zone to be transferred to authorised secondary
nameservers via AXFR/IXFR, restricted to the configured slave
addresses (with optional TSIG). AXFR from an unauthorised source SHALL
be refused.

#### Scenario: Authorised AXFR succeeds

- **WHEN** a transfer request arrives from a configured `SecondaryNs`
        address
- **THEN** the zone is transferred and the slave is notified on changes.

#### Scenario: Unauthorised AXFR refused

- **WHEN** a transfer request arrives from an address not in
        `SecondaryNs`
- **THEN** the request is refused with `DnsError::AxfrForbidden`.

### Requirement: Glue Records for Vanity NS

The system SHALL let an authorised caller add glue records (A/AAAA) for
delegated vanity nameservers of a zone.

#### Scenario: Glue added

- **WHEN** an Admin `PUT /dns/zones/{z1}/glue` with a vanity NS and its
        `glue_ip`
- **THEN** the glue A/AAAA record is created and served for the NS
        delegation.
