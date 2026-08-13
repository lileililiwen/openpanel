# Add per-site WAF rules

## Why

Baota and cPanel both ship a "firewall" panel for web traffic: bad
bots, geo blocking, rate limits, simple signature rules. OpenPanel
already has a host firewall module (iptables/nftables via
`host-security`), but no per-site request filtering. Without a WAF, a
single site on the panel can be hammered with no per-site knob to
turn. This change adds a `waf` bounded context that compiles
typed rule sets into nginx `limit_req`, `limit_conn`, `geo`,
`map`, and `if` blocks — generated into the existing per-site nginx
config, validated with `nginx -t`, and rolled back on failure. The
panel never edits nginx by hand.

## What Changes

- New `waf` bounded context with a `RuleSet` aggregate per site
  composed of typed rules: rate-limit, connection-limit, geo-block,
  user-agent block, path block, header challenge, body-size cap.
- Rules are validated against a JSON schema; the panel rejects
  unknown rule kinds and unknown fields.
- Rules are compiled to an nginx snippet by a typed
  `NginxSnippetCompiler`; the snippet is concatenated into the
  per-site `server { … }` block during the existing render pass.
- The same atomic-write + `nginx -t` + reload discipline the
  sites module uses applies here: a bad rule never reaches the
  live config.
- Per-rule hit counter and last-trigger timestamp exposed through
  the existing monitoring time series and surfaced in the web UI.
- REST, CLI, and `/sites/{id}/waf` web surface.

## Capabilities

### New Capabilities

- `waf`: per-site rule sets, snippet compilation, hit metrics.

### Modified Capabilities

- `sites`: the per-site nginx render now inlines the WAF snippet
  before `location` directives.

## Impact

- Domain: `RuleSet` aggregate, typed rule enum
  (`RateLimit`, `ConnLimit`, `GeoBlock`, `UserAgentBlock`,
  `PathBlock`, `HeaderChallenge`, `BodySizeCap`).
- App: `WafService`, `NginxSnippetCompiler` (typed port), hit
  counter sampler.
- API/CLI/web: `/api/v1/sites/{id}/waf`, `openpanel waf
  {rules,add,remove,enable,disable}`, `/sites/{id}/waf` page.
- The compiler never reads or writes the live config; it returns
  the snippet as a string and the sites module concatenates it.
