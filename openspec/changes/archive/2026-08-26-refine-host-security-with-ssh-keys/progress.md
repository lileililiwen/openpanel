# Progress — refine-host-security-with-ssh-keys

**Done (session 2026-08-26):**

- Domain: `HostSshKey`/`KeyAlgo` pure parser (single line, known algo
  first token rejects embedded options, per-algo minimum body sizes
  reject rsa-1024), SHA256 fingerprints, managed-block renderer
  preserving out-of-band lines; units + 100-case property.
- App: `HostSshKeysService` over `host_ssh_keys` (SECURITY_V002,
  unique fingerprint), atomic authorized_keys writer (tmp 0600 ->
  rename -> stat assert) with `SshKeyChanged` audit, last-used
  recorder; `SecurityModule::with_firewall_and_keys` for sandboxed
  tests.
- REST: GET/POST `/api/v1/host/ssh-keys`, DELETE `/{id}` (Admin/Owner;
  duplicate -> 422). CLI: `openpanel ssh-keys {list,add,remove}`.
- Tests: integration crud/guards/duplicate/audit; CLI E2E add -> list
  -> managed file -> remove. Workspace suite green.

**Remaining:** 1.6 journal-line fixture test (recorder shipped);
5.2 blocked environmentally (docs-gate OOM); deferred web page +
live-SSH smoke; archive.