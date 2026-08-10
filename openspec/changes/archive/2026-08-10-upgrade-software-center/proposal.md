## Why

`add-software-center` shipped the foundations — typed catalog entries, plan
digests, job lifecycle, a fixed-argument APT adapter, signed catalog
verification, and a per-app one-click deployer. The page renders correctly,
but the catalog itself is a hardcoded `recovery_catalog()` function and the
page lists it as a flat bullet list with raw "Review install" buttons. That
is acceptable as a security kernel, not as a product: an Owner cannot search
the catalog, cannot pick a version, cannot see categories or tags, and cannot
tell at a glance which software is installed. The catalog has no relation to
anything published on the network.

Real control panels — Baota, aaPanel, cPanel — all run an aggregator model:
they fetch a versioned, signed catalog from a vendor CDN, render a rich
storefront with category tabs, full-text search, and per-app detail pages,
and they use the panel as a *consumer* of recipes, not as a source of truth.
OpenPanel's job here is to fetch a remote data-only manifest, verify it, and
present it; the manifest itself is curated and published upstream. Catalog
content must therefore be:

- **Remote-first.** The default catalog URL is a configurable signed feed; the
  embedded recovery seed exists only for offline bootstrap when no remote
  snapshot has been activated yet.
- **Rich.** Every entry carries tags, a category, a license, a developer, a
  homepage, a multi-version recipe list, declared dependencies and conflicts,
  a short and long description, and provenance (the source that published
  it).
- **Discoverable.** The UI exposes full-text search, category tabs, tag
  filtering, installed-only and update-available filters, and sort order.
- **Version-aware.** Every install lets the Owner pick a specific version.
  Compatibility is checked up front (PHP runtime, supported OS, declared
  conflicts) before a plan is generated.
- **Wizard-driven.** Application deployment is a multi-step form (version →
  site → PHP runtime → database → review) so the Owner never has to remember
  a flag set.
- **Production-typed.** Card-grid storefront, detail page with tabs, live job
  progress, and a refresh-now button that shows source, last refresh, expiry,
  and entry count.

The previous `add-software-center` work stays the security kernel (plan
digests, confirmation tokens, APT adapter, RBAC, CSRF, audit, redaction,
recovery). This change builds the aggregator, the data model, the search,
and the storefront on top of it.

## What Changes

- Replace `recovery_catalog()` as the rendering source with a remote
  signed-manifest aggregator; keep the embedded function as the
  *offline-bootstrap seed* used only when no remote snapshot has ever been
  activated.
- Extend the recipe schema with categories, tags, multi-version recipe lists,
  dependencies, conflicts, license, developer, homepage, descriptions, and
  per-version install profiles (packages, artifact URL + digest, PHP
  requirement, disk size, supported OS list).
- Add a `CatalogSource` port: `RemoteHttpCatalogSource` fetches the
  configured URL, `EmbeddedCatalogSource` serves the bootstrap seed, and the
  service tries remote first and falls back to embedded only when no active
  snapshot exists.
- Add a `refresh_catalog()` action with a typed `CatalogRefresh` plan (URL,
  timeout, expected signature), a manual "Refresh now" button on the UI,
  diagnostics showing last refresh time, age, source, and entry count, and a
  staleness warning when the active snapshot is older than a configured
  threshold.
- Add server-side search and filtering: full-text on name/description/tags
  via SQL `LIKE` over a normalized `software_entries` view; category and tag
  filters; installed-only and update-available toggles; sort by name, recent
  refresh, or popular.
- Add per-version selection: every install requires a chosen version; the
  pre-flight check rejects the plan early when the host OS, architecture, or
  selected PHP runtime cannot satisfy the recipe.
- Add an install wizard for applications: a five-step form (version → site
  → PHP runtime → database → review) with a digest-bound confirmation at the
  end, sharing the existing plan/token/execute pipeline.
- Replace the flat list on `/software` with a production storefront: top
  category tabs, a search input, a card grid showing icon, name, version,
  short description, status badge, and a primary action, a detail page with
  Overview / Versions / Changelog / Dependencies / Source tabs, a "Refresh
  catalog" button, and a bounded live job-progress panel.
- Expand the embedded seed catalog to a realistic Baota-style set (Nginx,
  Apache, OpenLiteSpeed; PHP 7.4 / 8.0 / 8.1 / 8.2 / 8.3 / 8.4; MySQL
  5.7 / 8.0, MariaDB 10.x / 11.x, PostgreSQL 14 / 15 / 16, Redis, Memcached,
  MongoDB; phpMyAdmin, Adminer; WordPress 6.x, Drupal 10/11, Joomla, Ghost,
  Typecho, Nextcloud, Matomo, BookStack, Halo, Discourse, Gitea, Mattermost)
  with proper tags, categories, multi-version entries, and pinned
  digests/URLs.
- Add REST endpoints, CLI commands, and web routes for the new search,
  filter, sort, refresh, version-selection, and wizard actions.
- Add catalog ingestion tests: signed manifest acceptance, expired/tampered
  rejection, embedded fallback when offline, refresh-time persistence, and
  last-known-good retention on refresh failure.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `software-center`: the existing capability gains the aggregator model,
  the rich recipe schema, search/filter, version selection, install wizard,
  production storefront, and an expanded seed catalog. The trust boundary
  (Ed25519 signature, schema, expiry, fail-closed refresh) is preserved; the
  embedded `recovery_catalog()` becomes a recovery seed only, used when no
  remote snapshot has been activated.

## Impact

Adds a `CatalogSource` port, a remote HTTP fetcher, an embedded fallback
source, normalized catalog persistence, search/filter indexes, a wizard
controller, a refreshed CSS bundle, and new REST/CLI/web surfaces. Refresh is
exposed manually in v1; a periodic background task is gated on operator
opt-in and runs through the same `CatalogRefresh` plan pipeline. No new
external services are required to run the panel — the URL is configurable,
and the embedded seed keeps the page useful when the network is unavailable.
