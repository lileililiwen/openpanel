# per-site-php-runtime Specification

## Purpose

TBD - created by archiving change add-per-site-php-runtime.

## Requirements

### Requirement: PHP Runtime Reference

The bounded context SHALL model a `PhpRuntimeRef` carrying: `package_id` (e.g. `php-fpm`), `version` (e.g. `8.3.0`), `socket_path` (under `/run/php/`), `owner` user id, and `status` (`Running | Stopped | Unknown`). The constructor SHALL reject empty `package_id` or `version`, and SHALL reject any socket path that does not fall under `/run/php/`.

#### Scenario: Invalid runtime reference

- **WHEN** `PhpRuntimeRef::new("php-fpm", "8.3.0", "/tmp/php.sock", user_id)` is called
- **THEN** the constructor returns `PhpRuntimeError::InvalidRuntime`.

#### Scenario: Valid runtime reference

- **WHEN** `PhpRuntimeRef::new("php-fpm", "8.3.0", "/run/php/php8.3-fpm-test.sock", user_id)` is called
- **THEN** the constructor succeeds and `status` defaults to `Unknown`.

### Requirement: FPM Pool Spec

The bounded context SHALL model a `PhpFpmPoolSpec` carrying `site_id`, `runtime`, `pool_name`, `config_path` (under `/etc/php/<ver>/fpm/pool.d/`), `written_at`, and `last_pool_status`. The constructor SHALL derive the pool name from the site id and the config path from the runtime version.

#### Scenario: Pool config under FPM pool.d

- **WHEN** a `PhpFpmPoolSpec` is built for site_id=`{...}` and runtime version=`8.3.0`
- **THEN** `config_path` starts with `/etc/php/8.3.0/fpm/pool.d/` and the pool name starts with `openpanel-`.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit audit events for assign / swap / clear with the site id, the runtime ref, and the actor.

#### Scenario: Runtime swap is audited

- **WHEN** an operator swaps a site's PHP runtime
- **THEN** the audit event records both runtime refs and the site id
        without socket paths. The bounded context as archived today owns the typed model + persistence; the audit + service layer ships in the follow-on change.

### Requirement: Disambiguation

The `HostingPlan.allowed_php_runtimes` (in the `hosting-plans` bounded context) SHALL carry a `HostedPhpRuntimeRef` — a NEW string identifier for the *runtime family* the plan allows. The `PhpRuntimeRef` in this bounded context is the *deployed runtime* (package id, version, socket path). The two types are distinct and may share version strings but do not collide.

#### Scenario: Family ref and deployed ref stay distinct

- **WHEN** a plan allows the `php` family and a site deploys
        `php@8.3`
- **THEN** the plan's `HostedPhpRuntimeRef` and the site's
        `PhpRuntimeRef` remain different types that never compare
        equal despite sharing version text.
