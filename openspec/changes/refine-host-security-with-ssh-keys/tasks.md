# Refine Host security with SSH keys — Tasks

## 1. Testing

- [x] 1.1 Unit: key validation — valid ed25519 line accepted;
      rsa-1024 rejected; multi-line input rejected; embedded options
      in submitted line rejected (`InvalidKey::OptionsEmbedded`);
      duplicate fingerprint rejected.
- [x] 1.2 Unit: managed-block rendering — two keys render inside
      markers with panel options prepended and label comment appended;
      out-of-band lines preserved verbatim below the block; block hash
      changes when entries change.
- [ ] 1.3 Unit: atomic write — simulated write failure leaves the
      original file untouched (temp+rename semantics).
- [x] 1.4 Property: for arbitrary valid key sets (≥100 cases) rendered
      file contains each fingerprint exactly once, every managed line
      starts with the enforced options, and no private-key material
      can appear (input is structurally incapable of it).
- [ ] 1.5 Integration (`tests/integration/host_ssh_keys.rs`): POST
      valid key → 201 + GET lists it with fingerprint; DELETE → gone
      and rendered file updated; non-Admin caller → 403; audit events
      present for both mutations.
- [ ] 1.6 Integration: last-used parser fed a fixture journal line
      updates `last_used_at` for the matching fingerprint only.
- [ ] 1.7 CLI E2E: `cli_host_ssh_key_add_then_list_then_remove`.
- [ ] 1.8 Web: SSH Keys page at 360/768/1280 px; screenshots.

## 2. Domain

- [x] 2.1 Add `HostSshKey`, pure parser/validator, renderer under
      `crates/openpanel-domain/src/host_security/`.

## 3. Application

- [ ] 3.1 Repo + migrations; authorized_keys writer (atomic, 0600,
      post-stat assert); fingerprint shell-out reuse.
- [ ] 3.2 Service CRUD + audit; background last-used matcher.

## 4. Adapters and UI

- [ ] 4.1 REST routes (Admin-gated).
- [ ] 4.2 CLI subcommands.
- [ ] 4.3 Web page.

## 5. Validation

- [ ] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: add a real key, SSH in with it, confirm login
      works and `last_used_at` populates after the matcher runs.
- [ ] 5.4 Archive with
      `openspec archive refine-host-security-with-ssh-keys`.
