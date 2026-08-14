# Add OS Update Management — Tasks

## 1. Testing

- [x] 1.1 Unit: parse `apt-get -s upgrade` output; classify security
      vs other by origin; derive reboot-required from kernel package
      presence.
- [x] 1.2 Property: the apply request only ever resolves to
      allow-listed `apt` arguments; no free-form shell string reaches
      the executor.
- [x] 1.3 Service: list updates, apply-security, set policy; audit
      each mutation.
- [x] 1.4 Integration: apt dry-run lists updates; policy file is
      written to the unattended-upgrades config path.
- [ ] 1.5 Web: OS Updates panel shows security vs other, reboot flag,
      and an apply-security button.

## 2. Domain and Application

- [x] 2.1 Implement `PackageUpdate`, `UpdatePolicy`, `RebootState`,
      `UpdateKind` under `crates/openpanel-domain/src/os_updates/`.
- [x] 2.2 Add SQLite migration for `update_history`, `update_policy`.
- [x] 2.3 Implement `UpdateLister`, `UpdateApplier`,
      `UnattendedConfig`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/admin/os/updates`, `/admin/os/updates/apply`,
      `/admin/os/updates/policy` routes (Admin-gated).
- [ ] 3.2 Build the OS Updates panel (list, classify, apply, policy
      form).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: list updates, apply-security, confirm history
      row + reboot flag reflects a kernel update.
- [x] 4.4 Archive with `openspec archive add-os-update-management`.
