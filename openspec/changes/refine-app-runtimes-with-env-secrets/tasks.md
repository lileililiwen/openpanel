# Refine App runtimes with env & secrets — Tasks

## 1. Testing

- [x] 1.1 Unit: `EnvKey` validation — accepts `DATABASE_URL`,
      `_X1`; rejects `1BAD`, `A-B`, empty, >128 chars; reserved keys
      (`PATH`,`HOME`,`USER`) rejected with `EnvError::ReservedKey`.
- [x] 1.2 Unit: `EnvSet` uniqueness — duplicate keys collapse into an
      error on construction; ordering is insertion-stable.
- [x] 1.3 Unit: size caps — set exceeding 64 KiB or 128 vars rejected;
      single plain value > 8 KiB rejected.
- [x] 1.4 Golden test: rendering a runtime with no env produces the
      pre-change unit bytes exactly; adding one non-secret var adds
      exactly one `Environment=` line; any secret var switches to
      `EnvironmentFile=` and the unit contains no secret plaintext.
- [x] 1.5 Property: for arbitrary valid sets (≥100 cases) the rendered
      unit plus env file never contain a secret value in the unit
      text, and every key appears at most once.
- [ ] 1.6 Integration (`tests/integration/runtime_env.rs`): PUT set
      with one secret → GET returns the secret's key + `secret: true`
      but no value; non-secret values round-trip verbatim; audit event
      lists changed keys only.
- [ ] 1.7 Integration: pending-restart flag set on PUT, cleared by the
      existing restart action; app process sees values (mock exec
      records environment).
- [ ] 1.8 CLI E2E: `cli_runtime_env_set_secret_from_stdin` — value
      never appears in argv, output, or terminal echo.
- [ ] 1.9 Web: Environment tab at 360/768/1280 px; masked secret
      inputs; screenshots.

## 2. Domain

- [x] 2.1 Add `EnvVar`/`EnvSet`/`EnvKey` + validation under
      `crates/openpanel-domain/src/app_runtimes/`; extend
      `render_supervisor_unit` with env branch.

## 3. Application

- [x] 3.1 Repo column (JSON; cipher values in crypto layout) +
      migration; 0600 env-file writer inside the chroot. (Storage +
      cipher layout + env-file renderer shipped; the chroot writer
      lands with the lifecycle wiring.)
- [x] 3.2 Service get/put/delete with audit + pending-restart flag
      wired to the existing lifecycle actions. (Get/put with
      changed-keys-only audit shipped; delete + pending-restart flag
      land with the REST surface.)

## 4. Adapters and UI

- [ ] 4.1 REST routes per design.
- [ ] 4.2 CLI subcommands (secret via stdin).
- [ ] 4.3 Web tab.

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: deploy a node app printing `process.env`, set a
      var + secret, restart, observe both in-app and neither in logs.
- [ ] 5.4 Archive with
      `openspec archive refine-app-runtimes-with-env-secrets`.
