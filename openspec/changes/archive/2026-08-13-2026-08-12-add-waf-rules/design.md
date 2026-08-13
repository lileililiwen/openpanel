# Add per-site WAF rules — Design

## Domain model

```
RuleSet
  { site_id, version, rules: Vec<Rule>, default_action:
    Allow | Challenge | Deny }

Rule
  { id, kind, enabled, priority, params, hit_count,
    last_triggered_at? }
```

`kind` is a closed enum:

```
RateLimit       { zone, rate (r/s), burst, nodelay }
ConnLimit       { zone, per_ip }
GeoBlock        { countries: ["CN","RU"], action:
                  Deny | Challenge }
UserAgentBlock  { pattern, action }
PathBlock       { pattern, method?, action }
HeaderChallenge { header, value, challenge:
                  Tarpit | Redirect(path) }
BodySizeCap     { max_bytes }
```

## Snippet compilation

The `NginxSnippetCompiler` consumes a `RuleSet` and produces a
`String` snippet. Example for a single rate-limit:

```nginx
# openpanel-waf site=42 rev=7
limit_req_zone $binary_remote_addr zone=site42_rl:10m rate=10r/s;
limit_req zone=site42_rl burst=20 nodelay;
limit_req_status 429;
```

Geo blocks compile to a `map $geoip_country $site42_block { default 0; … }`
plus a `if ($site42_block) { return 403; }`. The compiler emits one
`map` per (site, kind) and re-uses it across rules.

## Render integration

The sites module's render pass now calls the WAF compiler and
inlines the result before any `location` block:

```nginx
server {
  server_name example.com;
  # … existing per-site config …
  # openpanel-waf site=42 rev=7
  limit_req_zone $binary_remote_addr zone=site42_rl:10m rate=10r/s;
  limit_req zone=site42_rl burst=20 nodelay;
  # …

  location / { … }
  location ^~ /.well-known/acme-challenge/ { … }
}
```

If `nginx -t` fails on the rendered file, the panel restores the
previous config (same pattern as today for site blocks) and surfaces
the error.

## Hit metrics

A periodic sampler reads nginx's stub_status or the panel-side
access log (configured) and increments per-rule counters. The
counters flow into the monitoring time series as
`waf.hits{kind,site,action}`. Existing alert rules can subscribe
to sudden spikes via the notification channel pipeline.

## Validation

- JSON schema for `params` per `kind`; unknown fields are rejected.
- The compiler is pure: given the same `RuleSet`, the same snippet
  comes out byte-for-byte (idempotent and testable).
- `nginx -t` is mandatory before any reload; failure is a typed
  `WafCompileError` returned to the API.

## Endpoints

```
GET    /api/v1/sites/{id}/waf
PUT    /api/v1/sites/{id}/waf        { default_action, rules: [...] }
GET    /api/v1/sites/{id}/waf/hits
POST   /api/v1/sites/{id}/waf/test   { rule } → compile preview (no
                                       write)
```

`PUT` is a single atomic write: the whole ruleset is replaced
and the snippet is recompiled. There is no incremental edit because
the priority ordering and zone allocation depend on the whole set.

## Tests

```
1.1  Unit: JSON schema validation per rule kind; compiler
     produces a stable, byte-identical snippet for the same
     ruleset.
1.2  Property: every rule kind compiles to a snippet that
     `nginx -t` accepts; unknown fields are rejected; an empty
     ruleset produces an empty snippet.
1.3  Service tests with mock nginx validator and mock monitoring
     for hit ingestion and hysteresis.
1.4  Integration: full render → `nginx -t` → reload → hit a path
     that matches a rule → counter increments.
1.5  CLI E2E: `openpanel waf {rules,add,remove,enable,disable,
     test}` with a real `nginx -t` invocation.
1.6  Web: /sites/{id}/waf with rule list, add form (CSRF),
     test-rule button, and hit counter chart.
```
