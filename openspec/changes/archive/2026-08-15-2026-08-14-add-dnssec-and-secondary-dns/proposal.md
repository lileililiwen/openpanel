# Add DNSSEC and secondary DNS

## Why

The archived `add-dns-zone-management` change lists DNSSEC key custody
and secondary DNS as explicit Non-Goals. The active
`refine-dns-with-zone-templates` change adds zone templates only, leaving
the authoritative-signing and redundancy story incomplete. Competitors
that OpenPanel benchmarks against — cPanel, Plesk, HestiaCP, and
Panelica — all ship DNSSEC. Without DNSSEC, OpenPanel cannot offer
tamper-proof DNS or meet the expectations of security-conscious
multi-tenant hosts; without secondary DNS, zones have a single point of
failure. This change adds DNSSEC signing and secondary/AXFR DNS.

## What Changes

- Per-zone DNSSEC signing with ZSK/KSK separation, CDS/CDNSKEY rollover,
  and DS publication to the registrar.
- Secondary DNS / AXFR zone transfer to slave nameservers.
- Glue records for vanity NS (delegated nameservers).
- New endpoints: `/dns/zones/{id}/dnssec` (enable/rotate),
  `/dns/zones/{id}/secondary`, `/dns/zones/{id}/glue`.
- Coupling: depends on `dns` capability and
  `refine-dns-with-zone-templates`; DS publication couples to `ssl` for
  registrar/publishing flows.

## Capabilities

### Modified Capabilities

- `dns`: add per-zone DNSSEC signing (ZSK/KSK, CDS/CDNSKEY rollover,
  DS publishing) and secondary DNS (AXFR) plus glue records for vanity
  NS.

## Impact

- Domain: `DnsSecPolicy`, `ZoneSigningKey`, `SecondaryNs`,
  `GlueRecord`, `DsRecord`.
- App: `DnsSecService`, `KeyRolloverEngine`, `AxfrSender`,
  `GlueRecordService`.
- API/CLI/web: `/dns/zones/{id}/dnssec`,
  `/dns/zones/{id}/secondary`, `/dns/zones/{id}/glue`; CLI
  `openpanel dns {dnssec,secondary,glue}`; web DNS zone tabs.
- Security: private KSK/ZSK held in key custody, never exported in API
  responses; AXFR restricted to authorised slave IPs.
- Coupling: depends on `dns`; DS publication interacts with `ssl`
  (registrar publish). `refine-dns-with-zone-templates` supplies the
  zones these operations target.
