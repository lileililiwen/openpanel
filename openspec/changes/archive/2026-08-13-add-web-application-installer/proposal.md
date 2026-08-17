# Add web application installer

## Why

`refine-software-center-with-app-classification` typed the
`WebApplicationManifest` but did not implement the installer.
Baota's "App Store" and cPanel's "Installatron-lite" both let
operators install a typed application (WordPress, Ghost,
Joomla, phpBB) under a site's document root with its own DB
and config overlay. This change implements the install path,
idempotency, dry-run preview, and post-install overlay hook.

## What Changes

- New bounded context `web-application-installer` exposing
  `WebApplicationInstaller` and a per-app `RecipeRunner`.
- New endpoints: `GET /web-apps`, `POST /web-apps/install`
  (preview), `POST /web-apps/install/run`,
  `POST /web-apps/{id}/upgrade`, `DELETE /web-apps/{id}`
  (uninstall).
- New SQLite tables: `web_app_installs(site_id, app_id, version,
  install_path, db_ref, secret_ciphertext, config_overlay_paths,
  installed_at)`.
- Idempotency: re-running the same install on the same site
  refuses unless `idempotency_key` is fresh; concurrent runs
  are serialized per `(site_id, app_id)`.

## Capabilities

### New Capabilities

- `web-application-installer`: installable web applications with
  preview, run, upgrade, uninstall.

## Impact

- Domain: `WebApplicationId`, `InstallPlan`,
  `InstallRun`, `InstalledWebApp`, `IdempotencyKey`,
  `SecretCiphertext` (encrypted under master key).
- App: `WebApplicationInstallerService` in
  `crates/openpanel-app/src/web_application_installer/`.
- API/CLI/web: `/api/v1/web-apps/*`; CLI
  `openpanel webapp {install,upgrade,uninstall,list}`; web
  `/web-apps` page.
- Coupling: depends on `software-center` for catalog and
  signature verification, `databases` for DB creation, `sites`
  for chroot.
