# software-center Specification

## Purpose
TBD - created by archiving change add-software-center. Update Purpose after archive.
## Requirements
### Requirement: Trusted Software Catalog and Discovery
The system SHALL expose an Owner-readable catalog of typed system components and web applications with categories, descriptions, supported versions/platforms, dependencies, licenses, provenance, and lifecycle state. The catalog MUST use an embedded trust root, accept remote manifests only after signature/schema/expiry validation, and classify detected host software without silently taking ownership.

#### Scenario: Catalog signature is invalid

- **WHEN** a remote catalog has an invalid signature, unsupported schema, or expired validity window
- **THEN** the system rejects it, retains the last-known-good catalog, records a redacted audit event, and executes no recipe content

#### Scenario: Existing Nginx was installed externally

- **WHEN** discovery finds a compatible Nginx package not installed by OpenPanel
- **THEN** the catalog reports it as externally managed and neither changes nor removes it without an explicit adoption transaction

### Requirement: System Component Planning and Lifecycle
Owners SHALL preview, install, adopt, update, configure, and uninstall allowlisted system components, initially Nginx, supported PHP-FPM versions/extensions, MySQL or MariaDB, and Redis. Every mutation MUST execute a fresh digest-bound plan through a typed package-manager adapter and SHALL report dependencies, conflicts, downloads, disk impact, affected services, configuration ownership, and rollback capability before confirmation.

#### Scenario: Install PHP version and extensions

- **WHEN** an Owner confirms a valid plan for PHP 8.x with selected allowlisted extensions
- **THEN** the system installs fixed packages, validates PHP-FPM, registers its service, makes the version assignable to sites, and records a secret-free completed job

#### Scenario: Plan changed after preview

- **WHEN** package state or the catalog changes after a preview token was issued
- **THEN** execution rejects the stale token and requires a new preview rather than applying a different transaction

#### Scenario: Dependency remains in use

- **WHEN** an Owner previews removal of Nginx, PHP, MySQL, or Redis while a managed capability still depends on it
- **THEN** the plan reports exact aggregate dependency counts and blocks removal unless a supported migration removes those dependencies

### Requirement: Safe Package Execution and Recovery
The system SHALL serialize host package transactions, use fixed executable paths and arguments, respect native package-manager locks, bound and redact output, snapshot OpenPanel-owned configuration, validate services after changes, and roll back when the recipe declares rollback support. Interrupted jobs SHALL be reconciled on startup and MUST never be reported as successful without post-install validation.

#### Scenario: Package validation fails

- **WHEN** package installation finishes but the component's configuration or readiness validation fails
- **THEN** the job enters rollback, restores owned configuration and package state where supported, leaves the last-known-good services active, and exposes a redacted failure

#### Scenario: Concurrent package request

- **WHEN** another host package job is already active
- **THEN** a new mutation is queued or rejected as busy and no second package-manager process starts

#### Scenario: Panel restarts during installation

- **WHEN** OpenPanel starts and finds a non-terminal job without a live owned process
- **THEN** it marks the job interrupted and offers only recipe-supported retry or rollback actions

### Requirement: One-click Web Application Deployment
Owners SHALL deploy supported pinned releases of WordPress and Drupal from the Software Center by selecting a domain, site settings, PHP version, database options, locale, and optional DNS, TLS, and backup integration. Deployment MUST verify official artifacts, reject unsafe archive entries, use least-privilege credentials, validate application health, and unwind only job-created resources on failure.

#### Scenario: Deploy WordPress successfully

- **WHEN** an Owner confirms a compatible WordPress plan for an unused domain
- **THEN** the system creates the site and database, verifies and extracts the pinned artifact, writes secret-safe configuration, validates the installation, optionally provisions DNS/TLS/backup hooks, and returns generated administrator credentials once

#### Scenario: Drupal requires unavailable PHP extension

- **WHEN** a Drupal release requires a PHP extension not present in the selected runtime
- **THEN** preview reports the missing dependency and offers an explicit combined plan rather than beginning a partial deployment

#### Scenario: Archive contains traversal entry

- **WHEN** an application artifact contains an absolute path, parent traversal, escaping symlink, excessive file count, or expanded-size violation
- **THEN** extraction fails before writing outside staging and all resources created by the deployment are rolled back

### Requirement: Software Jobs, Surfaces, and Observability
REST, CLI, and Owner-only `/software` web surfaces SHALL provide catalog search, installed state, previews, job progress/history, confirmation, cancellation, retry/rollback, component lifecycle, and WordPress/Drupal deployment. Browser mutations MUST enforce CSRF. Responses, logs, audits, and diagnostics MUST exclude repository credentials, application secrets, database passwords, environment secrets, and unbounded package output.

#### Scenario: Non-owner attempts installation

- **WHEN** an Admin or User requests a component or application mutation through any surface
- **THEN** the system returns forbidden before catalog downloads, package commands, filesystem writes, or resource creation

#### Scenario: Owner watches installation progress

- **WHEN** an Owner opens an active job
- **THEN** the panel shows bounded phase-level progress and redacted diagnostics without exposing commands or secrets

#### Scenario: Cancel during unsafe phase

- **WHEN** an Owner requests cancellation while the package manager is in a non-interruptible phase
- **THEN** the system records cancellation as pending and transitions only at the next recipe-declared safe checkpoint

### Requirement: Remote Catalog Aggregator and Refresh

The Software Center SHALL aggregate its catalog from one or more remote
signed data-only manifests fetched at runtime and persist the active
snapshot in a normalized store. The system MUST reject any manifest whose
Ed25519 signature, schema, expiry, canonical payload, origin, or size fails
validation, retain the last-known-good snapshot, record a redacted audit
event, and execute no recipe content. The default catalog URL is
configurable (`OPENPANEL__SOFTWARE__CATALOG_URL`); the embedded
`recovery_catalog()` function exists only as the offline-bootstrap seed and
MUST be materialized into the same normalized store on first boot when no
remote snapshot has ever been activated. A manual `refresh_catalog` action
is exposed to Owners and reuses the existing durable transaction lock so
that no refresh can interleave with a package install.

#### Scenario: First boot with reachable remote feed

- **WHEN** OpenPanel starts and the configured catalog URL is reachable
  with a valid signed manifest
- **THEN** the manifest is verified, the active snapshot is set, the page
  renders entries from the snapshot, and the embedded seed is never
  materialized

#### Scenario: First boot with unreachable feed

- **WHEN** OpenPanel starts and the configured catalog URL is unreachable
  or returns a non-2xx response
- **THEN** the embedded seed is materialized into the normalized store,
  the page renders the seed entries, and a refresh-failure audit event is
  recorded

#### Scenario: Refresh of tampered manifest

- **WHEN** an Owner triggers a refresh and the manifest payload differs
  from its signature, the schema is unsupported, the expiry is past, the
  origin is not on the allowlist, or the payload exceeds 1 MiB
- **THEN** the refresh is rejected, the active snapshot is unchanged, the
  page shows the last-known-good entries, a redacted rejection audit event
  is recorded, and no recipe content is evaluated

#### Scenario: Refresh while a package job is active

- **WHEN** an Owner triggers a refresh while a package install or update is
  already running
- **THEN** the refresh waits for the durable transaction lock, runs after
  the active job, and is recorded as a separate job with its own progress
  phases

#### Scenario: Refresh progress is observable

- **WHEN** an Owner triggers a refresh
- **THEN** the page shows bounded phase progress (`fetching → verifying →
  applying → done`), the phase label is bounded in length, and the active
  snapshot continues to be served read-only until `applying` succeeds

### Requirement: Rich Recipe Schema and Provenance

Every catalog entry SHALL carry a stable identifier, a display name, a
closed `Category` (`OneClick`, `Runtime`, `Database`, `Cache`, `WebServer`,
`Mail`, `Tools`, `Security`, `Other`), at least one validated kebab-case
`Tag`, an SPDX `License` identifier, a `Developer` string, an HTTPS
`Homepage` URL, a short and long description, a closed `EntryKind` (`System`
or `Web`), at least one `VersionSpec`, a list of declared `dependencies`
and `conflicts` referencing other entry IDs, an install-size estimate in
bytes, a last-updated timestamp, and a `Provenance` record identifying the
catalog source URL, the manifest digest, and the activation time. Each
`VersionSpec` carries a per-version `InstallProfile` describing the fixed
package set (system entries) or the pinned artifact URL, digest,
`archive_root`, archive type, and supported PHP runtime versions (web
entries). Unknown fields MUST be rejected at activation.

#### Scenario: Manifest with unknown field is rejected

- **WHEN** a manifest entry contains a field not declared in the schema
- **THEN** the entire manifest is rejected, the active snapshot is
  unchanged, and the rejection is recorded in the audit log

#### Scenario: Entry without a license is rejected

- **WHEN** a manifest entry is missing `license` or carries a non-SPDX
  identifier
- **THEN** the manifest is rejected with a precise validation error and
  no entry from that manifest is activated

#### Scenario: Web entry without an artifact digest

- **WHEN** a `Web` entry's `InstallProfile` lacks a pinned URL, a digest,
  or `archive_root`
- **THEN** the manifest is rejected and the entry is never presented to
  the Owner

#### Scenario: Dependency on missing entry is reported

- **WHEN** an entry declares a `dependencies` reference to an entry ID
  absent from the same manifest
- **THEN** the manifest is rejected with a precise validation error
  naming the missing reference

#### Scenario: Per-version install profile is selectable

- **WHEN** an Owner opens a `Web` entry that declares multiple
  `VersionSpec`s, each with its own `InstallProfile`
- **THEN** the detail page lists every available version with its PHP
  requirements, disk size, and pinned digest, and the install flow uses
  the Owner-selected version's profile

### Requirement: Search, Categories, and Tags

The Software Center SHALL expose server-side full-text search over the
active snapshot's name, description, tags, category, and developer, plus
filter by single or combined `Category`, single or combined `Tag`, an
`installed-only` toggle, and an `update-available` toggle. Results SHALL
be sortable by name (default), most recent activation time, or supported
OS recency. The result set MUST be paginated with a cap of 60 entries per
page, and the response MUST NOT include secrets, package commands, or
provenance of entries the caller has not loaded.

#### Scenario: Search by free text

- **WHEN** an Owner types `wordpress` in the search box
- **THEN** the server returns every entry whose name, description, tag, or
  developer contains the query, with category and tag metadata, and a
  status badge reflecting the local install state

#### Scenario: Filter by category and tag

- **WHEN** an Owner selects the `Database` category tab and clicks the
  `mysql` tag
- **THEN** the server returns only `Database` entries tagged `mysql`,
  paginated, sorted by name

#### Scenario: Empty result has an empty state

- **WHEN** the search query or filter combination returns no entries
- **THEN** the page renders a non-empty empty state with the active query
  echoed and a `Clear filters` action, and never renders an empty
  `<ul>` or grid

#### Scenario: Update-available filter lists outdated software

- **WHEN** an Owner enables the `update-available` filter
- **THEN** the server returns only entries whose `latest_version` is
  newer than the locally installed version of the same entry, and hides
  entries with no local install

### Requirement: Per-Version Selection and Compatibility Check

Every install, update, or deployment action SHALL be initiated against an
explicitly chosen `VersionSpec` from the entry's recipe list. The pre-flight
check SHALL reject the action before any plan, lock, or side effect when
the chosen version is not supported on the detected host OS and
architecture, when the required PHP runtime is not installed, when a
declared `conflicts` entry is already installed and managed, or when the
declared `dependencies` cannot be satisfied. The Owner SHALL see a
typed compatibility report before the preview token is issued.

#### Scenario: PHP runtime missing for web app

- **WHEN** an Owner selects a WordPress 6.x version that requires PHP 8.1+
  and the host only has PHP 8.0 installed
- **THEN** the pre-flight check reports the missing PHP version and
  offers a combined plan that installs the required PHP version first;
  no preview token is issued for the unsupported plan

#### Scenario: Conflict with already-managed component

- **WHEN** an Owner selects a recipe that declares a `conflicts` reference
  to an entry the panel already manages (for example, `mariadb` while
  `mysql` is `panel_managed`)
- **THEN** the pre-flight check reports the conflict, refuses to start the
  plan, and links to the conflicting entry's detail page

#### Scenario: Unsupported host OS

- **WHEN** an Owner selects a recipe whose `platforms` list does not
  include the host's distribution and release
- **THEN** the entry is shown on the page with an `unsupported` badge, the
  install action is disabled, and the detail page explains the supported
  platforms

#### Scenario: Version pinned in plan

- **WHEN** an Owner confirms a plan for a chosen version
- **THEN** the plan digest is computed over the entry ID, the chosen
  `VersionSpec`, and the platform, and the execute step rejects the
  token when the catalog has changed since the preview

### Requirement: Install Wizard for Applications

Deployments of `Web` entries SHALL be driven by a five-step wizard: (1)
version selection, (2) site selection or `Create new site`, (3) PHP runtime
selection from `panel_managed` PHP entries, (4) database options
(autogenerate or attach existing), (5) review. Wizard state SHALL be held
in a signed, `HttpOnly`, `SameSite=Lax`, `Secure` cookie with a 10-minute
idle expiry; navigating back preserves already-collected fields; navigating
forward re-validates the current step. The final step calls the existing
`preview_deployment` with the assembled inputs and renders the same
digest-bound confirmation token the kernel already uses. The wizard SHALL
be Owner-only, CSRF-protected on every step, and SHALL never expose
package commands, repository credentials, or generated database passwords
outside the success response.

#### Scenario: Wizard step 2 lists existing sites

- **WHEN** an Owner reaches step 2 of the WordPress wizard
- **THEN** the page lists every existing site as a radio option plus a
  `Create new site` option, and refuses to advance when the chosen site is
  missing a `public_html` directory or has uncommitted state

#### Scenario: Wizard rejects domain collision early

- **WHEN** an Owner reaches step 2 and the chosen site already has files
  in its document root
- **THEN** the wizard reports the collision in step 2 and refuses to
  advance to step 3, without issuing a preview token

#### Scenario: Wizard step 3 lists only managed PHP runtimes

- **WHEN** an Owner reaches step 3 of the WordPress wizard
- **THEN** the page lists only `panel_managed` PHP entries from the
  inventory, ordered by version, and refuses to advance when the chosen
  PHP version is not present

#### Scenario: Wizard assembles and previews

- **WHEN** an Owner completes steps 1..4 and submits step 5
- **THEN** the server calls `preview_deployment` with the assembled input,
  renders the confirmation page with the affected services, packages,
  disk impact, and the digest-bound confirmation token, and exposes no
  other plan metadata

#### Scenario: Wizard state survives a refresh

- **WHEN** an Owner refreshes the tab between wizard steps
- **THEN** the signed cookie restores the previously-collected fields and
  the current step, and rejects the request when the cookie signature is
  invalid or the idle expiry has passed

### Requirement: Production Software Center Storefront

The `/software` page SHALL render a storefront with: (a) a top category tab
row driven from the active snapshot's distinct categories, (b) a search
input, (c) an `installed-only` toggle, an `update-available` toggle, a sort
selector, and a `Refresh catalog` button, (d) a card grid where each card
carries an inline-SVG icon, the entry name, the latest version, the
license, a one-line description, a status badge (`available`,
`installed`, `update-available`, `externally-managed`, `unsupported`), and a
primary action button (`Install`, `Update`, `Adopt`, `Remove`, or `Deploy`)
wired to the wizard or to the appropriate preview route, (e) a job-progress
panel listing the most recent non-terminal jobs with bounded phase labels,
(f) a catalog-diagnostics strip showing source URL, last refresh time,
entry count, and signature status. The page MUST degrade gracefully without
JavaScript: the search, filter, sort, and tab controls work as form GETs.
The CSS SHALL be additive to the existing `app.css` and SHALL define the
`.storefront`, `.card`, `.wizard`, and `.storefront__detail` families with
both light and dark themes.

#### Scenario: Card grid shows install / update / remove per state

- **WHEN** an Owner visits `/software` and the catalog contains Nginx
  (`installed`), Redis (`available`), and MariaDB (`externally-managed`)
- **THEN** Nginx renders an `Update` and `Remove` action, Redis renders
  `Install`, MariaDB renders `Adopt`, and each card shows the appropriate
  status badge and version

#### Scenario: Tab navigation drives server-side filter

- **WHEN** an Owner clicks the `Database` tab
- **THEN** the URL is updated to `/software?category=Database` and the
  server returns only `Database` entries, paginated

#### Scenario: Detail page lists versions and dependencies

- **WHEN** an Owner opens a WordPress detail page
- **THEN** the page renders Overview, Versions, Changelog, Dependencies,
  and Source tabs, lists every available version with its pinned digest
  and PHP requirement, lists declared dependencies with their entry IDs
  and current state, and lists the catalog source URL and activation
  timestamp under Source

#### Scenario: Job progress is bounded and redacted

- **WHEN** an active job is shown in the progress panel
- **THEN** the panel shows the bounded phase label, the plan digest, the
  elapsed time since the last state transition, and a `Cancel` action,
  and the rendered HTML MUST NOT contain package commands, raw
  arguments, or secret material

#### Scenario: Stale catalog is visible

- **WHEN** the active snapshot is older than the configured staleness
  threshold (default 24 hours)
- **THEN** the diagnostics strip shows a non-blocking warning with the
  snapshot age, and the `Refresh catalog` button is highlighted

### Requirement: Catalog Diagnostics and Staleness

The Software Center SHALL expose, on the `/software` page and through
`software diagnostics` and a new `software refresh` command, the source
URL of the active snapshot, the activation timestamp, the manifest
digest, the entry count, the signature verification status, the most
recent refresh attempt outcome, and the age of the active snapshot. When
the snapshot is older than the configured `OPENPANEL__SOFTWARE__CATALOG_MAX_AGE`
(default 24 hours) the page SHALL show a non-blocking warning, and a
`Refresh catalog` button SHALL be available everywhere the diagnostics
strip is shown. The diagnostics response MUST NOT include command output,
raw headers, or the rejected envelope bytes.

#### Scenario: Diagnostics reflects last refresh attempt

- **WHEN** the most recent refresh attempt failed signature verification
- **THEN** the diagnostics strip shows `signature: rejected`, the
  activation timestamp of the still-active previous snapshot, the
  rejection timestamp, and a link to the audit log entry

#### Scenario: Manual refresh from the UI

- **WHEN** an Owner clicks `Refresh catalog`
- **THEN** the panel POSTs to a CSRF-protected `software refresh` route
  that runs the refresh plan, streams bounded progress through the job
  panel, and either activates the new snapshot or leaves the active
  snapshot untouched with a typed error

#### Scenario: CLI diagnostics match the UI

- **WHEN** an Owner runs `openpanel software diagnostics`
- **THEN** the CLI prints the same JSON shape the diagnostics strip uses,
  with the same source URL, age, entry count, and signature status

### Requirement: Least-privilege install path

The Software Center SHALL NOT require the panel process to run as
root. When the panel is not root, every package-tool invocation
that requires privilege SHALL be wrapped in `sudo -n` (non-
interactive) over a fixed allowlist. The allowlist is:

```
openpanel ALL=(root) NOPASSWD: /usr/bin/apt-get, /usr/bin/dnf, /usr/bin/yum, /usr/bin/zypper, /usr/bin/pacman, /sbin/apk
```

Read-only commands (dpkg-query, rpm, nginx `-t`, mysqladmin
`ping`, etc.) SHALL NOT be wrapped in `sudo`. The wrapper
SHALL detect root by reading `/proc/self/status` and parsing
the `Uid:` line, NOT by calling `getuid()`. The wrapper SHALL
be `unsafe`-free (the workspace lints forbid `unsafe` blocks).

When `sudo -n` rejects the request, the wrapper SHALL return
`SoftwareCenterError::Package` whose detail includes the
exact `/etc/sudoers.d/openpanel` entry the operator must
install, the `visudo -c && systemctl restart sudo` reload
command, and a pointer to this spec.

#### Scenario: Panel is not root, sudo allowlist is missing

- **WHEN** the panel process has `uid != 0` and the operator
  has not installed the allowlist in `/etc/sudoers.d/openpanel`
- **THEN** `sudo -n /usr/bin/apt-get -y install -- apache2`
  exits non-zero with `sudo: a password is required`, the
  wrapper returns `Package("… failed: …. The panel is not
  running as root and `sudo -n` (non-interactive) rejected the
  request. Add the following line to
  /etc/sudoers.d/openpanel on this host and reload sudo
  (visudo -c && systemctl restart sudo):\n\nopenpanel
  ALL=(root) NOPASSWD: /usr/bin/apt-get, /usr/bin/dnf,
  /usr/bin/yum, /usr/bin/zypper, /usr/bin/pacman,
  /sbin/apk\n\nOr run the panel as the root user (not
  recommended).")`, the web shell renders a typed error
  page with the same text, and the install is recorded as
  `failed` in the audit log with the sudo rejection message.

#### Scenario: Panel is not root, sudo allowlist is installed

- **WHEN** the panel process has `uid != 0` and the operator
  has installed the allowlist in `/etc/sudoers.d/openpanel`
- **THEN** `sudo -n /usr/bin/apt-get -y install -- apache2`
  exits zero, the install succeeds, the audit log records
  `installed` with the actor, the entry id, and the host
  platform, and the storefront surfaces the success.

#### Scenario: Panel is root

- **WHEN** the panel process has `uid == 0`
- **THEN** the wrapper short-circuits to the inner command;
  no `sudo` is invoked, the install runs as root directly, and
  the audit log records the install with `actor=root` and
  the host platform.

#### Scenario: Read-only command (dpkg-query) does not escalate

- **WHEN** `HostPackageManager::discover` runs
  `/usr/bin/dpkg-query -W -f=…` on a non-root panel
- **THEN** the wrapper short-circuits to the inner command
  (no `sudo`), the command runs as the panel user, and
  the snapshot succeeds without prompting for a password.

### Requirement: Placeholder SHA-256 is fail-closed for live installs

The recovery seed is allowed to ship entries whose upstream
does not publish a hash (the `sha256` field equals the
all-zeros placeholder sentinel). For the live install path
(`install_artifact` and `HostPackageManager::apply`), the
Software Center SHALL refuse any install whose recipe
carries the placeholder digest, UNLESS the operator has set
`OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false` at
startup.

When the gate is on (the default), `place_artifact` SHALL
return `SoftwareCenterError::Invalid("digest placeholder is
not allowed when OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS
is true")` for any Web entry whose `pin.sha256` is the
placeholder sentinel. The Install button for that entry
SHALL be disabled in the storefront and the entry detail
page SHALL explain that the operator must run
`software refresh` against a remote catalog that pins a real
digest before installing.

The gate is a startup-time check (one env-var read at
boot), not a per-install check.

#### Scenario: adminer seed ships a placeholder digest

- **WHEN** the recovery seed for adminer carries
  `sha256: "0000…0000"` and the gate is on
- **THEN** `install_artifact` for adminer returns
  `Invalid("digest placeholder is not allowed when
  OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS is true")`,
  the storefront renders the Install button as disabled,
  and the entry detail page shows the message above the
  Versions tab.

#### Scenario: opt-in via env var for air-gapped recovery

- **WHEN** the operator sets
  `OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false`
- **THEN** the install proceeds with the placeholder skipped
  (existing behavior, `digest_verified: false` is reported)
  so the operator can install from the embedded recovery
  seed on a host with no network access to a remote
  catalog.

### Requirement: No user input in shell commands

The install command for every `Family` SHALL be a fixed
string. The package name SHALL come from
`entry.versions[*].packages` (set by the seed), not from
the web form. The web form SHALL carry only `_csrf` (and
domain/php_version/locale for the WordPress-style wizard,
which goes through a separate path). The Install button
SHALL NOT have a `name`, `script`, or `path` field.

For Web entries, the URL and digest SHALL come from
`pin.url` and `pin.sha256` (set by the seed), not from the
web form. The destination path SHALL be
`webapps_root/<version>/<filename>`, derived from the
seed's `archive_type` and `archive_root`, not from user input.

#### Scenario: Click Install on adminer

- **WHEN** an Owner clicks Install on adminer
- **THEN** the panel POSTs to
  `/software/components/adminer/install` with only a
  `_csrf` field. The service looks up the entry, reads
  the URL and digest from the seed, downloads the file,
  verifies the digest (or skips for a documented
  placeholder under the opt-in env var), and lands the
  file at
  `/var/lib/openpanel/webapps/adminer/4.8.1/adminer-4.8.1-en.php`.
  No part of the install command, the URL, the digest, or
  the destination path comes from the web form.

### Requirement: Audit trail for every install

Every install (Web or System) SHALL record an `AuditEvent`
with the following fields:

- The entry id and display name.
- The actor (the user who clicked Install).
- For System entries: the package list and the
  `Family` used for the install.
- For Web entries: the source URL, the recipe digest,
  and `digest_verified`.
- The host platform (`/etc/os-release` `ID` +
  `VERSION_ID` + arch), captured at install time.
- The result: `installed`, `failed: <detail>`, or
  `rolled_back`.

The storefront and the entry detail page SHALL read the
last `installed` event per entry and surface it in a "Last
install" badge.

#### Scenario: Owner installs nginx

- **WHEN** an Owner installs the `nginx` System entry on
  an Ubuntu 24.04 host
- **THEN** the audit log records
  `actor=alice, entry=nginx, family=Apt,
  packages=[nginx], platform=ubuntu 24.04 x86_64,
  result=installed`. The entry detail page shows
  "Last install: alice · ubuntu 24.04 · succeeded".

#### Scenario: Owner installs adminer

- **WHEN** an Owner installs the `adminer` Web entry with
  the verified-digest gate off
- **THEN** the audit log records
  `actor=alice, entry=adminer, source_url=…,
  digest=0000…0000, digest_verified=false,
  platform=ubuntu 24.04 x86_64, result=installed`. The
  storefront shows the adminer card with "Last install:
  alice · digest unverified".

