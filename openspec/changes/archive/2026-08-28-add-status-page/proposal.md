# Add Status page

## Why

No surveyed panel ships native public status publishing — operators
bolt on Uptime Kuma or hosted status pages. OpenPanel already runs
synthetic checks (`SyntheticCheck`, `CheckResult`, `classify` in
`crates/openpanel-domain/src/synthetic_monitoring/mod.rs`) but results
are visible only to authenticated users via notification channels.
Publishing an opt-in public status page turns existing data into a
differentiator at near-zero marginal cost.

## What Changes

- Opt-in **public status page** per panel (slug-routed,
  unauthenticated): current check states, 90-day uptime bars derived
  from stored `CheckResult`s, incident history from status
  transitions.
- Per-check publish toggle; page-level enable/disable switch;
  configurable display labels that hide internal hostnames/ids.
- Anonymous endpoint rate-limited and cache-header'd; no secrets, no
  internal identifiers beyond chosen labels.
- Optional email subscription wired to the existing notifications
  dispatcher (double opt-in).
- Surfaces: public route `/status/<slug>`, admin API + web settings
  tab.

## Capabilities

### Modified Capabilities

- `synthetic-monitoring`: add public publishing of check states and
  history on top of existing checks/alerts.

## Impact

- Domain: `StatusPage{slug, enabled, entries[]}`, `PublishPolicy`,
  pure transition→incident derivation (`Incident{check, started_at,
  resolved_at}`).
- App: `StatusPageService`, SQLite repo, read-model projector from
  CheckResult history; rate-limit hook via tower middleware reuse.
- Web: public server-rendered page (tokens.css vocabulary, i18n-aware)
  plus admin settings tab; no new crate.
- Security: slug enumeration resistance (random slugs), no check
  target URLs exposed unless labelled, cache headers, audit on policy
  changes only (public reads unaudited by design).
- Coupling: synthetic-monitoring (data), notifications (subscriptions),
  web router/middleware, i18n/themeable-ui conventions.

## Non-goals

- No custom domains per status page (single panel slug).
- No historical backfill before adoption.
- No third-party embeds/scripts on the public page.
