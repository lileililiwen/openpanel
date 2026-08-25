# Refine App runtimes with env & secrets

## Why

The app-runtimes capability supervises node/python/ruby/go site apps,
but there is no way to give an app its environment: no env-var
management exists in specs or code, while the runtime spec even
promises logs contain "no environment secrets" that cannot exist yet.
Every peer panel with runtime support (Plesk Node.js extension,
CloudPanel apps, Easypanel Box service, Coolify/Dokploy env editing)
ships environment configuration; database-backed apps are unusable
without it.

## What Changes

- Per-runtime **environment variable management**: key/value list with
  validation (key charset, size caps), rendered into the supervisor
  unit's `Environment=`/`EnvironmentFile=`.
- **Secret values** flagged per variable: stored AES-256-GCM encrypted,
  write-only over API/CLI/web (never echoed back), redacted in logs.
- **Restart requirement**: changes take effect on next restart; the
  surface reports pending-vs-applied state.
- Surfaces: `GET/PUT /api/v1/sites/{id}/runtimes/{rt}/env`, CLI
  `openpanel site runtime env …`, web Runtime → Environment tab.

## Capabilities

### Modified Capabilities

- `app-runtimes`: add environment/secret configuration to the runtime
  lifecycle.

## Impact

- Domain: `EnvVar{key, value_cipher|plain, secret: bool}`, pure
  validation (`ENV_KEY_RE`, total-size cap), `RuntimeError::Env…`.
- App: extends `render_supervisor_unit`
  (`crates/openpanel-domain/src/app_runtimes/mod.rs:109-250`) with an
  `EnvironmentFile=` written 0600 into the chroot; crypto via existing
  AES-256-GCM helpers.
- API/CLI/web as above; DTO returns keys + secret flags only, never
  secret values.
- Security: secret plaintext appears only in the write request and the
  0600 env file inside the jail; audit events record key names and
  changed flags, never values.
- Coupling: app-runtimes module only; reuses identity of quota/cron
  patterns where needed.

## Non-goals

- No `.env` file import/export UI (manual paste later if demanded).
- No per-deploy ephemeral secrets or vault integration.
