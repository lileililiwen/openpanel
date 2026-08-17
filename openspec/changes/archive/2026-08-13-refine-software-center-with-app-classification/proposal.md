# Refine software-center with web-application classification

## Why

`openspec/specs/software-center/spec.md` focuses on system
components (Nginx, PHP-FPM, MySQL). Baota and cPanel differ from
OpenPanel here: they ship an **app store** for user applications
such as WordPress, Ghost, Joomla, phpBB, NextCloud. Without an
explicit classification, the follow-on `add-web-application-installer`
change will have no formal anchor in the existing catalog and the
type discrimination between "system" and "application" components
will be unclear. This refinement adds the classification and the
typed manifests without implementing the installer itself.

## What Changes

- New enum `ComponentKind { System, Application }` on every
  component descriptor.
- New field `target_site_type` on `Application` descriptors
  (LimitedTo{SingleSite, MultiTenant}).
- New manifest schemas: `system-component.json` (existing) and
  `web-application.json` (new). Both are signed with the same
  panel master key.
- `SoftwareCatalog` carries separate indices for each kind and
  refuses to install an `Application` through the `System`
  package adapter and vice versa.

## Capabilities

### Modified Capabilities

- `software-center`: typed classification; separate manifests;
  catalog indices.

## Impact

- Domain: `ComponentKind`, `TargetSiteType`,
  `WebApplicationManifest` value object.
- App: `SoftwareCatalog` adds two new types of entries; no
  installation behaviour change.
- Storage: `installed_components` gains a `kind` column with
  default `System` for back-compat.
