# Refine App runtimes with env & secrets — Design

## Explore & Reuse

- `crates/openpanel-domain/src/app_runtimes/mod.rs:109–250` —
  `SiteRuntime` + `render_supervisor_unit`: env support is a new pure
  branch of the existing renderer (golden-tested against current
  output).
- AES-256-GCM helpers (`crypto.rs` layout `hex(nonce):hex(cipher)`) —
  same primitive the ssl/identity modules use for secrets at rest.
- Lifecycle actions (start/stop/restart, Crashed status) from
  `openspec/specs/app-runtimes/spec.md` "Runtime Lifecycle Control" —
  pending-restart flag rides the existing restart action; no new task
  type.
- DTO/error conventions: `crates/openpanel-api/src/dto/site.rs`
  pattern; audit via `AuditService`.

## Model

```rust
pub struct EnvKey(String);                       // [A-Za-z_][A-Za-z0-9_]*, <=128
pub enum EnvValue { Plain(String), Secret(CipherText) }
pub struct EnvVar { key: EnvKey, value: EnvValue }
pub struct EnvSet { vars: Vec<EnvVar> }          // unique keys, insertion order
```

Caps: ≤128 vars, ≤64 KiB total plain+cipher length, single plain value
≤8 KiB. Reserved keys: PATH, HOME, USER, SHELL, LANG.

## Rendering

```
render_supervisor_unit(rt):
    ...existing...
    if rt.env.has_secrets():
        write jailed env file (0600, site-user owned)
        unit += "EnvironmentFile=<jail>/env/<rt-id>"
    else:
        for v in plain vars: unit += f"Environment=\"{k}={v}\""
```

Env file lines are `KEY=value`; secret values decrypted only at file
write time inside the app layer.

## Surfaces

```
GET /api/v1/sites/{id}/runtimes/{rt}/env     -> keys + secret flags (+plain values)
PUT .../env                                  -> full-set replace (validated)
DELETE .../env/{key}
CLI: openpanel site runtime env {set,unset,list}   # secret read from stdin
Web: Runtime → Environment tab (masked inputs for secrets)
```

Audit: `RuntimeEnvChanged{keys_changed, secret_flags_changed}` — names
only, never values.

## Layering

Domain: VOs + renderer branch (pure). App: repo column (JSON),
crypto, env-file writer, service. Adapters standard.
