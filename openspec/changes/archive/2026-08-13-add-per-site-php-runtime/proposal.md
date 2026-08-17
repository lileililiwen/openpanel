# Add per-site PHP runtime

## Why

`refine-sites-with-multi-php-clone-fields` introduced
`Site.php_runtime` as an optional `PhpRuntimeRef` field but did
not yet populate it. cPanel's MultiPHP Manager and Baota's
per-site PHP switcher let operators install multiple PHP
versions and assign any one to any site, with a live atomic
swap. This change implements the install / assign / swap
operations, the FPM pool management, and the audit trail.

## What Changes

- New bounded context `per-site-php-runtime` that consumes the
  existing `software-center` installer and emits FPM pool
  configurations into `/etc/php/<ver>/fpm/pool.d/`.
- New `PhpRuntimeRef` resolution: each ref points to an installed
  component with `{ package_id, version, socket_path, owner }`.
- New `PhpRuntimeService` with `assign(site, runtime)`,
  `unassign(site)`, `swap(site, runtime)`, `list-runtimes`,
  `list-pool-status`.
- Atomic swap: write the new pool config, reload PHP-FPM, and
  only then update `Site.php_runtime`. Concurrent requests are
  drained via FPM's graceful reload and a 5-second grace window
  in which both old and new sockets may receive traffic.

## Capabilities

### New Capabilities

- `per-site-php-runtime`: multi-PHP install / assign / swap.

## Impact

- Domain: `PhpRuntimeRef`, `PhpFpmPoolSpec`, `PhpRuntimeStatus`.
- App: `PhpRuntimeService` in
  `crates/openpanel-app/src/per_site_php_runtime/`, depending on
  the `software-center` package adapter.
- API/CLI/web: `POST /sites/{id}/php-runtime` (assign/swap),
  `GET /sites/{id}/php-runtime`, `GET /php-runtimes` (installed);
  CLI `openpanel site php {assign,swap,clear}`.
- Filesystem: writes under `/etc/php/<ver>/fpm/pool.d/` and
  reloads via `systemctl reload php<ver>-fpm`.
