## Context

DNS providers differ in identifiers, proxy flags, TTL constraints, and optimistic concurrency. Credentials are high-impact secrets. The panel must not imply it is authoritative when it only controls an external provider.

## Goals / Non-Goals

**Goals:** provider account lifecycle, zones/records, synchronization, safe validation, drift/conflict handling, propagation checks, and reusable DNS automation.

**Non-Goals:** authoritative DNS server, registrar/domain purchase, DNSSEC key custody, secondary DNS, or provider-specific features outside capability discovery.

## Decisions

1. Define provider-neutral `DnsZone`, `DnsRecord`, typed record data, `ProviderAccount`, and `RemoteVersion`. Provider adapters advertise supported record types, TTL bounds, proxy support, and permissions.
2. Encrypt provider credentials with AES-256-GCM under the master key. Responses return metadata and credential-test result only; logging/audit uses provider/account IDs.
3. Synchronization is explicit and read-before-write. Mutations carry the last observed remote version; conflicts return current remote state instead of overwriting. Imported remote records are not deleted automatically.
4. Record normalization handles FQDNs, canonical values, MX/SRV priority fields, TXT segments, and CNAME exclusivity. Destructive bulk replacement is not exposed in v1.
5. Site creation may propose, not silently apply, A/AAAA/CNAME records. DNS-01 obtains a scoped temporary TXT lease and cleans only the record it created.

## Risks / Trade-offs

- Provider compromise via stored token -> least-privilege guidance, encryption, redaction, rotation/test, and no retrieval.
- Eventual consistency -> distinguish provider acceptance from public propagation and poll bounded authoritative resolvers.
- Concurrent external edits -> remote-version conflict and explicit resync/retry.

## Migration Plan

Create empty tables; no existing sites are modified. Provider automation remains opt-in. Removing the module leaves externally created records untouched.
