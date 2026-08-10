## Why

OpenPanel can manage services after they already exist, but an administrator
still has to leave the panel and provision core hosting software and common web
applications manually. A control panel intended to replace aaPanel or cPanel
needs a safe, convenient catalog for installing supported runtimes and deploying
sites without turning the panel into an arbitrary root-script runner.

## What Changes

- Add an Owner-only Software Center with searchable categories, compatibility
  and installed-state discovery, version selection, and update visibility.
- Support curated system components including Nginx, supported PHP-FPM versions
  and extensions, MySQL/MariaDB, Redis, and other explicitly registered recipes.
- Support one-click web application deployment, initially WordPress and Drupal,
  integrated with sites, databases, DNS, SSL, backups, logs, and services.
- Add dry-run transaction previews, dependency and conflict checks, explicit
  destructive confirmation, bounded progress logs, cancellation, validation,
  rollback, and recovery after interrupted operations.
- Require versioned signed catalog manifests, checksummed artifacts, fixed
  package identifiers/arguments, supported operating-system policies, RBAC,
  CSRF, audit, and secret-safe one-time credential handling.
- Exclude arbitrary shell plugins, unreviewed third-party repositories, silent
  license acceptance, and automatic removal of externally managed software.

## Capabilities

### New Capabilities

- `software-center`: Curated system-component lifecycle and integrated one-click
  web application deployment.

### Modified Capabilities

None.

## Impact

Adds a software-catalog bounded context, transaction/job persistence, package
manager and application-deployer ports, signed catalog metadata, REST/CLI/web
surfaces, and composition hooks into sites, databases, DNS, SSL, firewall,
services, backups, and logs. Installation operations require Owner privileges
and host package-manager access.
