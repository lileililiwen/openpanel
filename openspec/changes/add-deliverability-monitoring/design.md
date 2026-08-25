# Add Email deliverability monitoring — Design

## Explore & Reuse

- `openspec/specs/mail/spec.md` — DKIM Keypair / Domain Sending Policy:
  auth-audit compares live DNS text records against these managed
  values (`crates/openpanel-app/src/mail/` already stores them).
- Notification dispatcher + webhook signing
  (`openspec/specs/notifications/spec.md`) — listing alerts are just
  new event kinds delivered by the existing pipeline.
- Cron job scope (`openspec/specs/cron/spec.md`) for scheduling;
  background-task precedent in monitoring module.
- Pure-parse precedent: WAF snippet compiler / sieve compile gate —
  the DMARC report parser follows the same "pure function +
  size-capped input" discipline.
- DNS resolution: new `ResolverPort` trait in domain, app-layer impl
  over a tokio-compatible stub resolver; tests inject a fake resolver
  (no network in tests).

## Model

```rust
pub struct BlocklistZone { pub zone: String }            // e.g. zen.spamhaus.org
pub struct Listing { ip: IpAddr, zone: String, first_seen, last_seen, resolved: Option<DateTime> }
pub struct AuthAudit { domain_id, spf_ok, dkim_ok, dmarc_ok, drift: Vec<String>, checked_at }
pub struct DmarcSourceStat { domain_id, source_ip, country: Option<String>,
                             messages: u64, dkim_pass: u64, spf_pass: u64, day: Date }
```

## Flows

```
check_domain(domain):
   ips = resolve(MX -> A) ∪ host outbound ips
   for zone in configured_zones:
       listed = resolver.txt(reversed(ip) + "." + zone) is A-record
       upsert Listing{first_seen preserved}
   if any new listing: notify(DeliverabilityListed{domain, zone, ip})

audit_auth(domain):
   fetch TXT records; compare SPF include-all, DKIM selector key hash,
   DMARC p=/rua= against managed state -> AuthAudit{drift[]}

ingest_dmarc_report(xml):
   reject len > 10 MiB; reject DOCTYPE/entities
   map rows -> DmarcSourceStat upserts (90-day retention prune)
```

## Endpoints / CLI / Web

```
GET  /api/v1/mail/domains/{id}/deliverability          (summary)
POST /api/v1/mail/domains/{id}/deliverability/check    (on-demand)
GET  /api/v1/mail/domains/{id}/deliverability/sources?days=30
PUT  /api/v1/settings/deliverability/blocklists        (zones list)
CLI: openpanel mail deliverability {check,show,sources,zones}
Web: Mail → Deliverability tab (status badges, listing table, source chart)
```

## Layering

Domain: pure types + report parser + validation. App: resolver impl,
scheduler, service, repo. Adapters standard. Tests inject
`MockResolver` and fixture XML — no network, repeatable.
