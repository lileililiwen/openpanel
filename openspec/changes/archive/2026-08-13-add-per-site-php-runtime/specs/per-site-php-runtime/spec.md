## Purpose

Implements per-site PHP runtime selection so that operators can
install multiple PHP versions side-by-side and assign any one
to any site, with a live atomic swap. Depends on the
`software-center` package adapter for installation and on the
`refine-sites-with-multi-php-clone-fields` change for the
`Site.php_runtime` field.

# per-site-php-runtime Specification

## Requirements

### Requirement: Installed PHP Runtime Registry

The system SHALL list installed PHP runtimes via `GET /api/v1/php-runtimes`. Each entry SHALL include `package_id`, `version`, `socket_template`, and `pool_status` (one of `Running`, `Stopped`, `Unknown`). The list SHALL be sourced by querying each installed `php-fpm` component in `software-center` and SHALL never include non-installed runtimes.

#### Scenario: Two installed runtimes

- **WHEN** an Owner has installed `php-fpm:8.2.27` and `php-fpm:8.3.18`
- **THEN** the response includes exactly those two entries with their pool statuses.

#### Scenario: Not installed

- **WHEN** no `php-fpm` components are installed
- **THEN** the response is an empty array; no placeholder or "install recommended" hint is returned.

### Requirement: Per-Site Assignment

The system SHALL let an authorised caller assign a runtime to a site via `POST /api/v1/sites/{id}/php-runtime` with a runtime reference `{ package_id, version }`. The service MUST verify the runtime is installed, create or update the FPM pool config, reload PHP-FPM, verify the socket via `GET /fpm-ping`, and on success update `Site.php_runtime`. On failure at any step, the prior pool config MUST be restored and PHP-FPM reloaded; the assignment MUST be rejected with a typed error.

#### Scenario: Successful assignment

- **WHEN** an Owner assigns `php-fpm:8.3.18` to site `s1`
- **THEN** `Site(s1).php_runtime == Some({8.3.18, ...})` and the socket responds to `/fpm-ping` within 2 seconds.

#### Scenario: FPM not responding

- **WHEN** `/fpm-ping` does not respond within 2 seconds
- **THEN** the assignment is rolled back, the prior pool config is restored, PHP-FPM reloaded, and the response is `PhpRuntimeError::FpmPingFailed{redacted}`.

### Requirement: Atomic Runtime Swap

The system SHALL support `POST /api/v1/sites/{id}/php-runtime/swap` with a fresh runtime reference. During the swap a 5-second grace window allows both sockets to receive traffic. The new runtime MUST pass `/fpm-ping`; if it fails after the grace window the swap is rolled back, the prior socket is restored, and `SwapRolledBack` is audited.

#### Scenario: Successful swap

- **WHEN** an Owner swaps `s1` from `8.2.27` to `8.3.18`
- **THEN** both sockets receive traffic during the 5s grace, only `8.3.18` serves traffic after, and `SitePhpSwapped{from=8.2.27, to=8.3.18}` is audited.

#### Scenario: New runtime fails ping

- **WHEN** after grace the new socket does not respond
- **THEN** the prior socket and Site state are restored; the API returns `SwapRolledBack{from, to, redacted_reason}`.

### Requirement: Runtime Clear

`DELETE /api/v1/sites/{id}/php-runtime` SHALL drain traffic to the existing socket, reload PHP-FPM and nginx without the pool, and set `Site.php_runtime = None`. Concurrent in-flight requests are processed by FPM's own graceful-stop timeout; the call returns within 30 seconds or reports `ClearTimeout`.

#### Scenario: Clear succeeds

- **WHEN** an Owner clears `s1`
- **THEN** `Site(s1).php_runtime == None` and the vhost no longer references the socket.

#### Scenario: Clear times out

- **WHEN** graceful stop exceeds 30 seconds
- **THEN** the call returns `ClearTimeout{redacted_reason}` and nginx remains pointing at the socket; audit `SitePhpClearTimeout`.

### Requirement: Plan and Hosting-Plan Coupling

A site MAY have a PHP runtime only if its owner's hosting plan (`add-hosting-plans`) lists that runtime as `allowed_php_runtimes`. The service MUST refuse to assign a runtime that the owner is not entitled to.

#### Scenario: Disallowed runtime

- **WHEN** `s1` is owned by a User whose plan disallows `8.3.18`
- **THEN** the assign call is rejected with `PlanDenied{axis=php_runtime, value=8.3.18}`.

#### Scenario: Allowed runtime

- **WHEN** the plan allows the runtime
- **THEN** assignment proceeds and the audit includes the plan id.
