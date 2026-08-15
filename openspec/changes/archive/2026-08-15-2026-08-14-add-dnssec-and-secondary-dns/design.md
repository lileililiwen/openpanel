# Add DNSSEC and secondary DNS — Design

## DnsSecPolicy model

```rust
pub struct DnsSecPolicy {
    pub zone_id: ZoneId,
    pub enabled: bool,
    pub algorithm: u8,            // e.g. 13 = ECDSAP256SHA256
    pub ksk_rollover_days: u32,
    pub zsk_rollover_days: u32,
}

pub struct ZoneSigningKey {
    pub zone_id: ZoneId,
    pub role: KeyRole,            // Ksk | Zsk
    pub tag: u16,
    pub public_blob: Vec<u8>,     // DS/CDS derivable; private kept in custody
    pub state: KeyState,          // Active | Published | Retired
}
```

## Secondary / glue models

```rust
pub struct SecondaryNs {
    pub zone_id: ZoneId,
    pub address: IpAddr,          // authorised AXFR source/dest
    pub tsig_name: Option<String>,
}

pub struct GlueRecord {
    pub zone_id: ZoneId,
    pub ns_name: String,          // vanity NS fqdn
    pub glue_ip: IpAddr,          // A/AAAA glue
}

pub struct DsRecord {
    pub zone_id: ZoneId,
    pub key_tag: u16,
    pub algorithm: u8,
    pub digest_type: u8,
    pub digest: Vec<u8>,
}
```

## Signing / rollover flow

```
enable_dnssec(zone):
  generate KSK + ZSK (key custody, private never in API)
  sign zone; publish DNSKEY + CDS/CDNSKEY
  compute DsRecord; offer publish to registrar (ssl coupling)

rotate_ksk(zone):
  generate new KSK; double-sign (Published->Active)
  publish new CDS; after parent DS swap, retire old KSK
```

## AXFR flow

```
secondary(zone, slave):
  restrict AXFR to SecondaryNs.address (+ optional TSIG)
  on zone change, notify slaves; serve AXFR/IXFR
```

## Endpoints

```
PUT  /api/v1/dns/zones/{id}/dnssec      body { enabled, algorithm, rollover_days }
POST /api/v1/dns/zones/{id}/dnssec/rotate body { role }
PUT  /api/v1/dns/zones/{id}/secondary   body { nameservers[] }
PUT  /api/v1/dns/zones/{id}/glue        body { glue[] }
```

## Tests

```
1.1 Unit: DS digest computation; KSK double-sign state machine; AXFR
    ACL match.
1.2 Property: private key never appears in API/serialized responses;
    glue IP resolves inside delegated NS.
1.3 Service tests w/ mock signer + mock registrar: enable, rotate KSK,
    publish DS.
1.4 Integration: zone signed serves RRSIG; AXFR to authorised slave
    succeeds; unauthorised AXFR refused.
1.5 CLI E2E: enable dnssec -> rotate -> add secondary.
1.6 Web: DNS zone tabs (CSRF), DNSSEC toggle, secondary/glue forms.
```
