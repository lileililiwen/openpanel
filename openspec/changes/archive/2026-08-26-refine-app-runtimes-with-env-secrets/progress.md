# Progress — refine-app-runtimes-with-env-secrets

**Done (session 2026-08-26):**

- Domain: EnvKey/EnvVar/EnvSet validation (reserved keys, uniqueness,
  insertion order, caps) + render_supervisor_unit_with_env with
  golden byte-identity for the no-env path; secrets ride
  EnvironmentFile and never appear in unit text; property over fuzzed
  sets.
- App: RuntimeEnvService over runtime_env (APP_RUNTIMES_V002) with
  AES-256-GCM ciphertext at rest, masked get views, changed-keys-only
  audit, env-file renderer.
- REST: GET/PUT /api/v1/runtimes/{id}/env.
- CLI: runtime-env {set,show} with secrets via stdin (never in argv).
- Tests: units, golden, property, secret-masking integration, CLI E2E.

**Remaining:** 1.7 pending-restart flag wiring to lifecycle actions;
deferred web Environment tab + live smoke; 5.2 full-workspace
double-run (interrupted by machine load); archive.