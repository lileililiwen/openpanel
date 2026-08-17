# Refine DNS with zone templates — Design

## Default templates

Three templates ship as compiled-in JSON, signed by the panel
master key on first run:

```jsonc
// strict (default for production)
{
  "apex":   [{ "kind": "A",     "ttl": 300 }],
  "www":    [{ "kind": "CNAME", "ttl": 300, "target": "@" }],
  "mx":     [{ "kind": "MX",    "ttl": 3600, "priority": 10 }],
  "spf":    [{ "kind": "TXT",   "ttl": 3600, "value": "v=spf1 -all" }],
  "dmarc":  [{ "kind": "TXT",   "ttl": 3600, "name": "_dmarc",
               "value": "v=DMARC1; p=reject; rua=…" }],
  "caa":    [{ "kind": "CAA",   "ttl": 3600, "value": "0 issue \"letsencrypt.org\"" }]
}

// relaxed (legacy compatibility)
"spf":     [{ "kind": "TXT",   "ttl": 3600, "value": "v=spf1 +a +mx ~all" }],
"dmarc":   [{ "kind": "TXT",   "ttl": 3600, "name": "_dmarc",
              "value": "v=DMARC1; p=none; rua=…" }]

// parked (apex parking page, no MX)
"apex":   [{ "kind": "A", "ttl": 300, "value": "<parking-ip>" }],
"mx":     []
```

DKIM keys are not part of the static template; they are generated
per-zone on first enable (RSA-2048 default, Ed25519 optional).

## Apply algorithm

```
enable(zone):
  template = ZoneTemplate::for_owner(owner)
  for record in template.render(zone):
    result = provider.create(zone, record)
    record.status = result.ok() ? Active : Pending(reason=redacted)
    append audit ZoneRecordApplied{record.kind, name, status}
  zone.status = Active
  persist atomically: all template records first, then zone status
```

Any record that fails provider validation is recorded as `Pending`
with the redacted reason; the zone is still `Active` because
`Pending` records do not block mail or TLS for other records.

## Endpoint

```
POST /api/v1/dns/zones/{id}/apply-template
  body: { template?: "strict" | "relaxed" | "parked",
          mode: "preview" | "apply",
          replace_existing: bool = false }
  → 200 (preview) { plan: [{ op, kind, name, value, conflict? }] }
  → 200 (apply)   { records: [{ id, kind, name, status }] }
```

`replace_existing=true` requires a typed `confirmed_at` field with
the current UTC timestamp to defeat replay and forces the caller to
be an Owner.

## Tests

```
1.1  Unit: template renderer for each kind; conflict resolver.
1.2  Property: every zone, on enable, has at least one A or AAAA
     record; every zone has an SPF record.
1.3  Service tests with mock provider: template apply under failure,
     retry, audit; replace_existing requires confirmation timestamp.
1.4  Integration: enable zone ⇒ all template records appear with
     Pending for provider-side failures.
1.5  CLI E2E: `openpanel dns apply-template --preview` produces
     correct plan JSON.
1.6  Web: zone detail page shows template records and a Re-apply
     Template button (CSRF).
```
