## Context

The previous `add-software-center` change established the trust boundary:
typed recipes, plan digests, one-use confirmation tokens, the exclusive
in-process and durable SQLite transaction locks, the fixed-argument APT
adapter, signed Ed25519 catalog envelopes, the bounded 8 KiB output with
secret-line redaction, the safe staged extraction, and the unwinding
application deployer. That work is the security kernel of the feature; this
change does not touch it. What it adds is the aggregator layer the panel
needs to feel like a real product.

aaPanel and Baota are the relevant references. Both publish a versioned,
signed, JSON manifest from a vendor CDN; the panel fetches it, stores it as
the active catalog, and renders a storefront. Each entry carries a category,
tags, a versioned recipe list, a license, a developer, a homepage,
screenshots, a description, dependencies, and a pinned artifact URL plus
checksum. The user picks a category tab, types a query in the search box,
clicks a card, lands on a detail page with Overview / Versions / Changelog /
Dependencies / Source tabs, picks a version, follows an install wizard, and
sees live job progress. Install scripts are an aaPanel implementation
detail and a security liability for us; OpenPanel keeps the panel in charge
of every command and reads the catalog as data only.

The remote feed is the source of truth; the panel aggregates and presents
it. When the feed is unreachable, the panel must keep working from its last
known good snapshot. When the panel is freshly installed with no network, a
tiny embedded recovery seed — Nginx, PHP-FPM, MySQL, MariaDB, Redis, plus
WordPress and Drupal — lets the Owner bootstrap the most common stack
without having to wait for a network round trip.

## Goals / Non-Goals

**Goals:**

- Remote-first catalog: a `CatalogSource` port, an `HttpCatalogSource`
  implementation, a default URL with a documented override knob, an
  embedded fallback for offline bootstrap, atomic activation, and
  last-known-good retention on refresh failure.
- Rich, versioned, typed recipe schema: categories, tags, license,
  developer, homepage, descriptions, per-version install profile (packages
  or artifact URL plus digest, PHP requirement, OS matrix, dependencies,
  conflicts, disk delta).
- Discovery UX: category tabs, full-text search, tag and installed-only
  filters, sort options, status badges, per-card install/update/adopt/remove
  action.
- Detail page: Overview / Versions / Changelog / Dependencies / Source tabs.
- Install wizard for applications: multi-step form that ends in the same
  digest-bound plan confirmation the kernel already uses.
- Production catalog seed: realistic Baota-style set covering common PHP
  versions, databases, web servers, runtimes, admin tools, and one-click
  applications.
- Diagnostics: source URL, last refresh time, entry count, signature
  status, staleness warning.

**Non-Goals:**

- Executing user-supplied shell snippets or catalog-supplied shell text
  (the catalog remains data-only; the panel keeps every command).
- Adding a marketplace where third parties can publish recipes without an
  explicit allowlist (the trust root stays single-signer in v1).
- Containerization, snap, Flatpak, or AppImage recipes (the host package
  manager stays the install path).
- Per-entry auto-update (the Owner is always the one who clicks "Update").
- Paid-license procurement or paid-app checkout.
- A built-in image gallery (screenshot URLs are stored and referenced; the
  panel does not host binary image assets in v1).

## Decisions

1. **Aggregator model.** `SoftwareCenterService` stops reading from
   `recovery_catalog()` directly. The service persists the active catalog
   in a normalized `software_entries` table with a `software_entry_versions`
   child table, populated by `activate_catalog(actor, role, source)` and
   read by `catalog(role)`. The recovery seed is materialized into the same
   tables on first boot only when no remote snapshot has ever been
   activated, so the service code path is identical regardless of source.

2. **CatalogSource port.** Two implementations. `HttpCatalogSource` does
   the actual fetch with a 30 s connect / 60 s read timeout, no redirects,
   origin allowlist (default `catalog.openpanel.dev`, configurable), and
   `User-Agent: openpanel/<version>`. `EmbeddedCatalogSource` returns the
   bootstrap seed. The port trait is `async_trait`, and the service always
   tries `HttpCatalogSource` first; on network error it logs an audit
   event and falls back to whatever is in the database (active snapshot
   from a previous successful refresh, or the embedded seed that was
   materialized on first boot).

3. **Refresh pipeline.** `refresh_catalog(actor, role, source, now)` is a
   typed plan with a digest (URL + filter), runs through the existing
   one-use confirmation flow when triggered from the UI, and executes
   asynchronously through the existing `SoftwareJob` state machine so the
   Owner can watch the bounded progress (`fetching → verifying → applying →
   done`). The kernel's exclusive transaction lock is reused, so no
   catalog refresh can interleave with a package install. A failed refresh
   leaves the active snapshot untouched; the rejected envelope and
   signature bytes are recorded in audit with redaction.

4. **Recipe schema.** New typed values: `Category` (closed enum
   `OneClick`, `Runtime`, `Database`, `Cache`, `WebServer`, `Mail`,
   `Tools`, `Security`, `Other`), `Tag` (validated kebab-case string,
   1..32 chars), `VersionSpec` (semver plus Ubuntu/Debian revision suffix
   with a strict parser), `EntryKind` (closed enum `System`, `Web`, `Tool`),
   `License` (SPDX identifier, validated), `Developer`, `Homepage`
   (validated HTTPS URL). Each entry has 1..N `VersionSpec`s, each with its
   own `InstallProfile` (system: package set with `--` end-of-options;
   web: artifact URL + digest + archive root + supported PHP versions).

5. **Search and filter.** Server-side. The active catalog is stored in
   `software_entries` with a generated `search_text` column containing
   `name || ' ' || description || ' ' || tags || ' ' || category || ' ' ||
   developer`. Search uses `WHERE search_text LIKE ? OR name LIKE ?`. Tag
   and category filters compose with `AND`. The card grid is rendered on the
   server; the search box uses a form GET so it works without JavaScript
   and degrades gracefully with HTMX. Pagination caps at 60 cards per page.

6. **Install wizard.** Five steps, each a separate route with server-side
   state held in signed cookies (not in-memory, so the Owner can refresh
   the tab). Steps: (1) version selection; (2) site selection (apps only,
   drawn from `sites` list, "create new site" option) or
   `component_choice` (system); (3) PHP runtime selection (apps only, drawn
   from `inventory` of `panel_managed` PHP entries); (4) database options
   (apps only — autogenerate or attach existing); (5) review + confirm.
   The final step POSTs to a new `wizard.confirm` route that calls the
   existing `preview_deployment` / `execute_deployment` with the assembled
   inputs. Going back to an earlier step preserves already-collected state.
   The wizard is rejected before any side effect when the chosen
   combination fails a compatibility check (missing PHP extension, OS not
   in the recipe's `platforms` list, site already exists, etc.).

7. **Storefront UI.** The `software_center` web module is rewritten on top
   of the existing `Shell` chrome. New CSS rules in
   `crates/openpanel-web/assets/app.css` add `.storefront`,
   `.storefront__tabs`, `.storefront__search`, `.storefront__grid`,
   `.card`, `.card__icon`, `.card__title`, `.card__meta`, `.card__status`,
   `.card__action`, `.storefront__detail`, `.wizard`, `.wizard__step`,
   `.wizard__nav`. Icons ship as inline SVGs in
   `crates/openpanel-web/src/software_center/icons.rs` (one SVG per
   Category) and are referenced by `data-icon`. Screenshots are URL-only
   in v1; the panel renders an `<img>` tag with `loading="lazy"`,
   `referrerpolicy="no-referrer"`, and the same origin allowlist as the
   artifact downloader.

8. **CLI parity.** `openpanel software refresh` runs the manual refresh.
   `openpanel software search <query>` and
   `openpanel software search --category <cat> --tag <tag>` compose. The
   `install`, `adopt`, `update`, `uninstall`, and `deploy` commands keep
   the same surface but accept `--version <v>` for component actions and a
   wizard-style flag set for deployments.

9. **Seed catalog.** The embedded `recovery_catalog()` is replaced with a
   real production set in
   `crates/openpanel-app/src/software_center/seed.rs`. The set covers the
   Baota staples: web servers (Nginx, Apache, OpenLiteSpeed), PHP runtimes
   (7.4 / 8.0 / 8.1 / 8.2 / 8.3 / 8.4), databases (MySQL 5.7 / 8.0,
   MariaDB 10.x / 11.x, PostgreSQL 14 / 15 / 16), caches (Redis, Memcached),
   tools (phpMyAdmin, Adminer, Web Terminal — the last is exposed as a
   category-tagged tool but never auto-installed), and one-click apps
   (WordPress 6.x with multiple versions, Drupal 10 / 11, Joomla 5,
   Ghost 5, Typecho, Nextcloud Hub, Matomo, BookStack, Halo, Discourse,
   Gitea, Mattermost). Every entry has at least one tag, a license, a
   developer, a homepage, a short and long description, and a per-version
   install profile with pinned URL + digest for web apps. The set is
   deliberately a starting point: the remote feed is the canonical
   catalog; the seed exists only to keep the panel useful when offline.

## Risks / Trade-offs

- **Remote feed compromise.** Mitigations: embedded trust root, signature
  verification, schema validation, expiry, `search_text` round-trip
  re-serialization check, max entry count, max payload size, no redirect,
  origin allowlist, fail-closed refresh, and an audit event recording the
  rejected envelope.
- **Recipe drift on the host.** The host may have installed a package
  outside the panel's managed set. The existing `inventory()` already
  classifies these as `externally_managed`; the storefront surfaces that
  as a status badge and disables the install / update action.
- **Stale catalog.** A staleness warning is shown when the active snapshot
  is older than the configured `OPENPANEL__SOFTWARE__CATALOG_MAX_AGE`
  (default 24 h). The Owner can still install from a stale catalog; the
  warning is informational, not blocking.
- **Search index bloat.** Bounded by the max entry count in the manifest
  and the 60-cards-per-page cap; the `search_text` column is computed once
  on activation and re-inserted rather than re-derived at query time.
- **Wizard state loss.** Held in a signed cookie (`HttpOnly`, `SameSite=Lax`,
  `Secure`); the wizard is recoverable across a refresh but expires after
  10 minutes of inactivity.

## Migration Plan

Ship the data model + remote-source first, behind the existing UI. The
existing `/software` page keeps rendering from the new normalized tables
without a visual change in step 1. The seed catalog is materialized on
first boot when no remote snapshot is present. The new storefront, search,
detail page, wizard, and refresh UI follow as a coordinated UI upgrade in
step 2. The recovery catalog function remains in the binary as the offline
seed; the page never reads from it directly.

## Open Questions

- Should the periodic refresh be a `cron` job in the panel or a supervised
  background task registered with `JobSupervisor`? v1 ships manual-only;
  the choice can be deferred until a real operator asks for it.
- Should the storefront offer a "popular" sort, and if so, is popularity a
  catalog-supplied number or derived from local install counts? v1 sorts
  by name and by most recent refresh; popularity is deferred.
- Does the seed catalog need to ship on every install, or only when the
  remote feed is unreachable? v1 ships the seed in the binary so offline
  bootstrap is immediate; an opt-out flag is unnecessary.
