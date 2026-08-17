# Add per-site PHP runtime — Design

## Runtime model

```rust
pub struct PhpRuntimeRef {
    pub package_id: SoftwareId,           // e.g. "php-fpm"
    pub version: SemVer,                  // e.g. 8.2.27
    pub socket_path: PathBuf,             // /run/php/php8.2-fpm-<site>.sock
    pub owner: UserId,
    pub pool_status: PhpRuntimeStatus,    // Running | Stopped | Unknown
}
```

Each ref maps to an installed component on the host; one ref
per `(version, site)` pair is enforced.

## Pool lifecycle

```
ensure_pool(site, runtime):
  pool_path = /etc/php/<v>/fpm/pool.d/<pool_name>.conf
  if exists and matches desired: return
  write pool config (user, group, socket, env, php_admin_values)
  systemctl reload php<v>-fpm
  verify sock responds to GET /fpm-ping within 2s
  if no ack: rollback by restoring prior pool config and reload

assign(site, runtime):
  ensure_pool(site, runtime)
  write nginx vhost snippet proxy_pass unix:<socket>
  nginx -t + nginx -s reload
  set Site.php_runtime = runtime
  audit SitePhpAssigned{site_id, runtime_id}
```

## Swap

```
swap(site, runtime_new):
  runtime_old = Site.php_runtime?
  ensure_pool(site, runtime_new)
  nginx reload accepts traffic to both sockets during grace
  wait grace window (5s) so any in-flight request finishes
  systemctl reload php<old_v>-fpm (now traffic moved)
  if new runtime fails health check, restore old socket +
  reload nginx; emit SwapRolledBack
  set Site.php_runtime = runtime_new
  audit SitePhpSwapped{from, to}
```

## Endpoints

```
GET    /api/v1/php-runtimes                    installed runtimes
GET    /api/v1/php-runtimes/{package_id}/{ver}/pool-status
POST   /api/v1/sites/{id}/php-runtime          body: { runtime_ref }
DELETE /api/v1/sites/{id}/php-runtime
POST   /api/v1/sites/{id}/php-runtime/swap     body: { runtime_ref }
```

## CLI

```
openpanel site php assign  <site_id> --runtime <pkg:ver>
openpanel site php swap    <site_id> --runtime <pkg:ver>
openpanel site php clear   <site_id>
openpanel php runtimes list
```

## Tests

```
1.1  Unit: PhpRuntimeRef validation; pool config builder;
      socket path uniqueness.
1.2  Property: only one runtime per (version, site); socket
      paths never collide within an installed component set.
1.3  Service tests with mock software-center adapter and mock
      systemctl: assign, swap, rollback, clear.
1.4  Integration: live swap with two installed PHP versions,
      end-to-end via the running panel (rolled back via test
      fixture on failure).
1.5  CLI E2E: install two PHP versions, assign, swap, clear.
1.6  Web: site detail page exposes PHP runtime picker with
      installed-versions list (CSRF).
```
