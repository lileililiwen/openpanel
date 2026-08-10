## Context

OpenPanel currently assumes that Nginx, MySQL, Redis, PHP-FPM, and similar
programs were installed outside the panel. The system-services module safely
controls a fixed inventory but deliberately cannot install packages. Sites and
databases already expose public application ports that a deployment
orchestrator can compose. Software installation runs with host privileges,
modifies shared package state, may restart services, and can leave a server
unusable when interrupted, so catalog provenance and transaction recovery are
more important than catalog size.

aaPanel combines infrastructure packages and one-click web programs in its App
Store. cPanel separates web-stack package profiles from application deployment.
OpenPanel will present one Software Center while retaining separate typed recipe
kinds and execution adapters internally.

## Goals / Non-Goals

**Goals:** provide searchable curated software, reliable host-state discovery,
reviewable install/update/uninstall plans, observable and recoverable jobs,
multiple supported PHP versions, and integrated WordPress/Drupal deployment.

**Non-Goals:** arbitrary distro package browsing, user-supplied shell scripts,
unreviewed plugins/repositories, containers in the first release, paid-license
procurement, unattended major upgrades, or replacing native package security
updates.

## Decisions

1. Use a versioned typed catalog. Entries are either `SystemComponent` recipes
   or `WebApplication` recipes. The binary ships a small recovery catalog;
   remote catalog updates require an Ed25519 signature, expiration timestamp,
   schema compatibility, and HTTPS. Unknown fields are rejected. This is safer
   than executing plugin code downloaded from a marketplace.
2. Support Debian/Ubuntu APT first behind a `PackageManager` port. Recipes map
   logical component/version/extension choices to fixed package identifiers and
   repository definitions owned by OpenPanel. The adapter never accepts command
   fragments from HTTP, catalog text, or users. DNF and containers can implement
   the same port later.
3. Separate planning from execution. A fresh preview records detected packages,
   conflicts, dependencies, downloads, disk impact, service disruption,
   configuration ownership, and rollback limitations. Execution requires the
   preview's short-lived token and exact digest, preventing time-of-check/time-of-
   use plan substitution.
4. Persist an exclusive host-package transaction and per-job state. Jobs move
   through queued, running, validating, rolling-back, and terminal states; logs
   are bounded and redacted. Startup reconciliation marks abandoned processes as
   interrupted and offers retry or rollback. Package-manager locks are respected,
   never bypassed.
5. Adopt rather than overwrite externally installed components. Discovery
   labels software as panel-managed, externally-managed, available, unsupported,
   or drifted. Adoption requires a compatible validation pass. Uninstall never
   removes external packages or a dependency still used by sites, databases,
   mail, backups, or another catalog entry.
6. Treat application deployment as an orchestrated transaction over public
   ports: reserve the domain, create the site and database, download a pinned
   official artifact, verify its digest/signature, extract through safe paths,
   write configuration with least privilege, validate the health endpoint, and
   optionally request DNS/SSL and backup registration. Failure unwinds only
   resources created by that job.
7. Return generated database/application administrator credentials once and
   store only existing encrypted/hash representations. Catalogs, job logs,
   audits, list/detail endpoints, and retries never contain credentials.
8. Offer REST, CLI, and Owner-only `/software` web flows. Every mutation uses
   CSRF for browsers, RBAC everywhere, explicit confirmation for removal or
   replacement, and an audit event containing recipe/job identifiers and plan
   digests rather than command output or secrets.

## Risks / Trade-offs

- [Catalog or artifact supply-chain compromise] → embedded trust root, signature,
  expiry, checksum, origin allowlist, and fail-closed refresh.
- [Package operation breaks active hosting] → dependency graph, service-impact
  preview, configuration backup, validation, rollback, and maintenance lock.
- [Distro/version fragmentation] → explicit platform matrix and APT-only first
  adapter; unsupported hosts remain read-only.
- [Application archive traversal or symlinks] → streaming extraction through the
  existing safe-path rules with entry, size, and file-count limits.
- [Rollback cannot downgrade every distro package] → label rollback guarantees
  per recipe and require confirmation when only forward recovery is possible.
- [Concurrent jobs corrupt package state] → one durable host transaction lock;
  independent application deployments can run only when their resource sets do
  not overlap.

## Migration Plan

Ship catalog browsing and host discovery first. Enable mutations only on a
recognized supported platform with a trusted catalog. Existing installations are
reported as external and remain untouched until explicitly adopted. The module's
SQLite tables are additive; disabling the module leaves installed software and
sites running. Rollback removes only resources created by a failed job and
restores OpenPanel-owned configuration snapshots.

## Open Questions

- Which additional Ubuntu/Debian releases enter the initial tested matrix?
- Should MariaDB be a separate catalog entry or an exclusive MySQL provider
  choice in the first catalog?
- Which trusted service will publish signed remote catalog updates?
