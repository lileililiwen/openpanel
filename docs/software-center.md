# Software Center

OpenPanel's Owner-only Software Center is available at `/software` and
through `openpanel software`. It is a **catalog aggregator** — a
production-grade storefront for curated system components and one-click
web applications — not a general distribution package browser and never
executes catalog-provided shell text.

## Aggregator model

The panel is a consumer. The remote signed catalog is the source of
truth, the panel aggregates and presents it. The catalog URL is
configurable:

```text
OPENPANEL__SOFTWARE__CATALOG_URL=https://catalog.openpanel.dev/v1/manifest.json
```

The remote feed is fetched on demand through a typed
`CatalogSource` port. The panel ships three implementations:

- **`HttpCatalogSource`** — TLS-only, redirect-free, origin-allowlisted,
  30 s connect / 60 s read timeout, 1 MiB cap. Used in production.
- **`EmbeddedCatalogSource`** — serves the offline bootstrap seed. Used
  on first boot when the remote feed is unreachable.
- **`StaticCatalogSource`** — serves an in-memory manifest. Used in
  tests and development with the deterministic fake adapter.

Every fetched manifest is verified against the embedded Ed25519 trust
root, schema, expiry, canonical JSON, and a strict allowed-origins list.
A failed refresh leaves the active snapshot untouched. The rejected
envelope and signature bytes are recorded in the audit log with
redaction.

The active snapshot is persisted in a normalized SQLite schema
(`software_entries`, `software_entry_versions`, `software_entry_tags`,
`software_entry_deps`, `software_entry_conflicts`,
`software_refresh_history`). Search, filtering, version selection, and
compatibility checks all read from this store.

## Storefront UI

The `/software` page is a production storefront with:

- **Header and diagnostics strip** — entry count, source URL, last
  refresh timestamp, signature status, staleness warning, and a
  `Refresh catalog` button.
- **Category tabs** — One-click, Runtimes, Databases, Caching, Web
  servers, Mail, Tools, Security, Other. The active category is a
  server-side `?category=` filter.
- **Search input** — full-text on name, description, tags, category,
  developer. Works as a form GET so the page works without JavaScript.
- **Filters** — `Installed only`, `Update available`, sort selector
  (Name / Recent / Size).
- **Card grid** — icon, name, latest version, license, one-line
  description, status badge (`Available` / `Installed` / `External` /
  `Unsupported`), and a primary action button (`Install` / `Update` /
  `Remove` / `Adopt`).
- **Job progress panel** — bounded list of recent non-terminal jobs
  with a `Cancel` action.
- **Empty state** — non-empty message with a `Clear filters` button
  when the query matches nothing.

The detail page at `/software/entries/{id}` shows Overview / Versions /
Changelog / Dependencies / Source tabs with metadata, the latest
version, every declared version, the dependency graph, declared
conflicts, and the catalog source / manifest digest.

The page degrades gracefully without JavaScript: search, filter, sort,
and tab controls work as form GETs. HTMX is used for the install
wizard and job progress panel for a snappier feel when available.

## CLI

```text
openpanel software catalog
openpanel software inventory
openpanel software search --query redis
openpanel software search --category database --tag mysql
openpanel software show --id wordpress
openpanel software preview --id redis
openpanel software install --id redis
openpanel software adopt --id nginx
openpanel software update --id redis
openpanel software uninstall --id redis
openpanel software deploy --application wordpress --domain example.com --php-version 8.3
openpanel software jobs
openpanel software cancel --job <id>
openpanel software retry --job <id>
openpanel software rollback --job <id>
openpanel software diagnostics
openpanel software refresh
```

`openpanel software search` returns a JSON page with `hits`, `total`,
`page`, `page_size`, and `sort`. `openpanel software diagnostics`
returns source URL, entry count, age, signature status, and last
refresh outcome.

## Production seed catalog

The embedded recovery seed is a real Baota-style set of 30+ entries
covering the most common hosting stack: web servers (Nginx, Apache,
OpenLiteSpeed), PHP runtimes (7.4 / 8.0 / 8.1 / 8.2 / 8.3), databases
(MySQL 5.7 / 8.0, MariaDB, PostgreSQL 16), caches (Redis, Memcached),
operator tools (phpMyAdmin, Adminer), and one-click applications
(WordPress, Drupal, Joomla, Ghost, Typecho, Nextcloud, Matomo,
BookStack, Halo, Discourse, Gitea, Mattermost). Every entry carries
a category, at least one tag, an SPDX license, a developer, a
homepage, a short and long description, and at least one version
with its own install profile. Web entries have a pinned `ArtifactPin`
with a published digest. WordPress carries both an SHA-1 and an
SHA-256 digest to match its own published verification chain.

The seed is build-time developer data and is the *last-resort*
fallback. The remote feed is the canonical catalog; the seed exists
to keep the page useful when offline.

## Recovery and security

Package transactions are serialized in-process and through a PID-aware
durable SQLite lock. A restart reconciles abandoned active jobs to
`interrupted` without taking a lock from a live panel process. Plans
and jobs are durable; confirmation tokens are stored only as SHA-256
hashes. Diagnostic output is bounded and secret-bearing lines are
redacted.

Application deployment returns generated administrator credentials
only in the success response and never in jobs, logs, audits, or
deployment records. Archive extraction rejects absolute/parent
paths, links, device entries, excessive expansion, excessive file
counts, and unexpected archive roots. The application adapter composes
the existing sites and databases ports and unwinds only job-created
resources in reverse order on failure.
