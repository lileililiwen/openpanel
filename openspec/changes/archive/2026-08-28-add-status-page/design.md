# Add Status page — Design

## Explore & Reuse

- `crates/openpanel-domain/src/synthetic_monitoring/mod.rs` —
  `SyntheticCheck` (:112), `CheckResult` (:184), `classify`
  (:230): the public page is a read-model over these; no new probe
  machinery.
- `openspec/specs/synthetic-monitoring/spec.md` — Define Check / Run
  Probe / Alert On Failure: publishing subscribes to the same result
  stream the alerter uses.
- `openspec/specs/notifications/spec.md` — Dispatcher + delivery
  semantics for email subscriptions (double opt-in reuses channel
  confirmation pattern).
- Web shell: `crates/openpanel-web/` router/layout/tokens.css; public
  page renders through the same layout minus auth chrome; i18n locale
  negotiation applies.
- Rate limiting: tower middleware already in scope via api stack;
  reuse for the anonymous route.
- Audit: policy mutations only (`StatusPagePolicyChanged`) — public
  reads are not audited.

## Read model

```rust
pub struct StatusPage { slug: Slug, enabled: bool,
                        entries: Vec<StatusEntry> }   // check_id, label, publish: bool
pub struct StatusEntry { label: String }              // label hides target URL
pub struct Incident  { check_id, started_at, resolved_at: Option<_>, outcome }

// pure derivation, unit-testable:
pub fn derive_incidents(results: &[CheckResult], warn: CheckStatus) -> Vec<Incident>;
pub fn uptime_bars_90d(results: &[CheckResult], day: Date) -> Vec<DailyBar>;
```

Slug generation: 128-bit random base32 (enumeration-resistant);
regeneration invalidates old URL.

## Public route

```
GET /status/<slug>
    unauthenticated; tower rate-limit (e.g. 60 req/min/IP)
    Cache-Control: public, max-age=30
    renders: per-entry current state badge, 90-day bars, open incidents
    disabled or unknown slug -> indistinguishable 404 page
GET /status/<slug>/subscribe   (POST email -> double opt-in via notifications)
```

Admin:

```
GET/PUT /api/v1/status-page            {enabled, slug regenerate}
PUT     /api/v1/status-page/entries/{check_id} {publish, label}
GET     /api/v1/status-page/incidents
CLI: openpanel status-page {show,enable,disable,publish,unpublish}
Web admin: Monitoring → Status page settings tab
```

## Rendering contract

tokens.css vocabulary only; single `<h1>`; state colours meet WCAG AA
contrast against tokens; mobile-first grid (bars stack <640 px); no
JS required. Literal-scan and template-literal gates apply.

## Layering

Domain: pure types + derivations. App: service/repo/projector.
Public route lives in openpanel-web (no auth deps); admin routes in
openpanel-api as usual.
