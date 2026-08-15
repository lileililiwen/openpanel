# Add non-PHP runtimes

## Why

OpenPanel today supports only PHP (via `per-site-php-runtime`) and
arbitrary Docker containers (via `container-runtime`). CloudPanel,
1Panel, Plesk, and cPanel all expose managed Node.js, Python, Ruby, and
Go runtimes natively; developers expect to pick a language and a version
pin for a site without hand-rolling a container. Without first-class
non-PHP runtimes, OpenPanel users must drop to Docker or manage
supervisor/process files by hand, losing per-site isolation, version
governance, and a single reverse-proxy story. This change adds an
`app-runtimes` bounded context that mirrors the `per-site-php-runtime`
pattern for the common languages.

## What Changes

- New bounded context `app-runtimes` carrying the `SiteRuntime`
  aggregate and `RuntimeService`, plus a per-user supervisor unit
  generator.
- Per-site runtime selection among Node.js, Python, Ruby, Go with a
  pinned version (e.g. `node:20`, `python:3.12`, `ruby:3.3`, `go:1.22`).
- Generate and manage a per-user supervisor unit that runs the app and
  keeps it alive; expose start / stop / restart and log tailing.
- Reverse-proxy: nginx routes `http(s)://domain` to the app's local
  port (default `127.0.0.1:APP_PORT`) instead of the PHP-FPM upstream.
- New endpoints: `GET /sites/{id}/runtime`,
  `PUT /sites/{id}/runtime`, `GET /sites/{id}/runtime/logs`.

## Capabilities

### New Capabilities

- `app-runtimes`: select a non-PHP runtime and version pin for a site,
  run it under a per-user supervisor unit, manage its lifecycle
  (start/stop/restart, logs), and front it with an nginx reverse proxy.

## Impact

- Domain: `SiteRuntime`, `RuntimeKind`, `RuntimeVersion`, `AppPort`,
  `RuntimeStatus`.
- App: `RuntimeService`, `SupervisorUnitBuilder`, `ReverseProxyLayer`.
- API/CLI/web: `/sites/{id}/runtime`, `/sites/{id}/runtime/logs`; CLI
  `openpanel site runtime {get,set,start,stop,restart,logs}`; web
  Runtime tab (CSRF).
- Security: runtime app runs inside the site chroot under the site
  user; supervisor unit is user-scoped; app port bound to loopback only
  and exposed solely through nginx.
- Coupling: depends on `sites` for chroot / docroot / user; follows the
  selection + per-version pattern of `per-site-php-runtime`;
  `container-runtime` remains an available alternative deployment path.
