## ADDED Requirements

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
