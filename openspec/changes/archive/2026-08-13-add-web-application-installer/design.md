# Add web application installer — Design

## Install flow

```
POST /api/v1/web-apps/install
  body: { app_id, site_id, idempotency_key?, dry_run?: true }

  plan = installer.preview(app_id, site_id) // typed InstallPlan
    - downloads: [{ url, sha256, size }]
    - extracts:  install_path
    - db:        { create: bool, db_name, db_user }
    - secrets:   [{ kind: "db_password" }]
    - overwrites: ["wp-config.php", ".env"]
    - warnings:  [...]
  return plan
```

```
POST /api/v1/web-apps/install/run
  body: { app_id, site_id, idempotency_key, plan_id, confirmed_at }

  if !confirmed_at.within(60s): 400 stale_confirmation
  if !matching_plan_id: 422 plan_changed
  else:
    download → verify sha256 → extract
    db.create_db()
    write config_overlay (template based)
    audit WebAppInstalled
    return { post_install_url, install_id }
```

## Concurrency

A single `(site_id, app_id)` lock is held by a `Mutex` in
`WebApplicationInstallerService`. Concurrent runs return
`409 Conflict{reason="install_in_flight"}` and the second
caller may poll the existing run.

## Secrets

DB passwords and other application-generated secrets are stored
encrypted in `web_app_installs.secret_ciphertext` (master key
wrapping). The plaintext is returned exactly once at install /
rotate and never re-displayed.

## Upgrade

```
upgrade(site_id, app_id, target_version):
  preview_new = installer.preview(target)
  if diff.overwrites.contains_non_overlay: confirm required
  upgrade-path: tar -cf backup + extract overlay
  audit WebAppUpgraded{from, to}
```

## Uninstall

Uninstalls are typed operations with a destructive
confirmation: a typed destructive token is required and the
DB / files are removed only on explicit `confirmed_at` within
60s and the database is dropped only if `drop_db=true`.

## Tests

```
1.1  Unit: idempotency key check; plan diff detector;
      secret rotation; preview/dry-run isolation.
1.2  Property: install plan's content_hash is stable across
      re-runs of the same input; rollback restores every
      byte.
1.3  Service tests with mock catalog and DB: install, upgrade,
      uninstall; concurrent run returns 409.
1.4  Integration: live install of "hello-world" fixture app on
      a test site.
1.5  CLI E2E: preview → run → upgrade → uninstall.
1.6  Web: install dialog (CSRF), upgrade confirmation, list.
```
