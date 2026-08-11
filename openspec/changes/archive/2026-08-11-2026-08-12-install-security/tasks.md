## 1. TDD and Security Tests

- [x] 1.1 Service-test the `PrivilegedCommand` wrapper. The
      wrapper SHALL prepend `sudo -n` when the program is in
      the privileged allowlist and the panel is not root. It
      SHALL short-circuit to the inner command when the
      program is not in the allowlist or the panel is root. —
      `crates/openpanel-app/tests/software_center.rs` adds
      `privileged_command_prepends_sudo_for_privileged_binaries`,
      `privileged_command_short_circuits_for_read_only_binaries`,
      `privileged_command_short_circuits_when_panel_is_root`,
      `privileged_command_surfaces_sudoers_snippet_on_failure`.
- [x] 1.2 Service-test the placeholder-digest fail-closed
      gate. With the gate on, `place_artifact` SHALL refuse
      any entry whose `pin.sha256` is the placeholder; with
      the gate off, the install proceeds and reports
      `digest_verified: false`. —
      `place_artifact_refuses_placeholder_digest_when_gate_is_on`
      and `place_artifact_allows_placeholder_when_opted_in`.
- [x] 1.3 Integration-test that the web install button is
      disabled in the storefront for an entry whose recipe
      carries a placeholder digest, and the entry detail
      page renders the operator-facing explanation. —
      `placeholder_digest_entry_renders_disabled_install_with_explanation`.
- [x] 1.4 Integration-test that the audit log records
      every install with the platform, the source URL, the
      digest, and the result. —
      `install_records_audit_event_with_source_digest_and_platform`.

## 2. Application and Domain

- [x] 2.1 Add `PrivilegedCommand` in
      `crates/openpanel-app/src/software_center/apt.rs` as
      a `PackageCommand` implementation that wraps an inner
      command and prepends `/usr/bin/sudo -n` for the
      privileged allowlist. Root detection reads
      `/proc/self/status` and parses the `Uid:` line. The
      wrapper is `unsafe`-free. Re-export from the
      `software_center` module root.
- [x] 2.2 Read `OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS`
      at startup. Default `true`. When the gate is on,
      `place_artifact` refuses any entry whose
      `pin.sha256` is the placeholder sentinel with
      `SoftwareCenterError::Invalid`.
- [x] 2.3 Extend `install_artifact` and
      `HostPackageManager::apply` to record an `AuditEvent`
      with the entry id, the source URL, the digest, the
      host platform, the actor, and the result. The
      storefront and the entry detail page read the last
      `installed` event per entry and surface it in a "Last
      install" badge.

## 3. Web UI

- [x] 3.1 Render the Install button as `disabled` for any
      Web entry whose recipe carries a placeholder digest
      and the gate is on. The entry detail page shows
      "This entry is not installed because the recovery
      seed does not pin a real SHA-256. Run `software
      refresh` against a remote catalog that pins a digest
      before installing." above the Versions tab.
- [x] 3.2 Surface the "Last install" badge on the storefront
      card and the entry detail page header. The badge
      reads from the audit log.

## 4. Validation and Delivery

- [x] 4.1 `cargo test --workspace` is green: every
      existing test still passes; the new tests in 1.1, 1.2,
      1.3, 1.4 pass.
- [x] 4.2 `make fmt` and `make clippy` are clean.
- [x] 4.3 Archive via `openspec archive`; commit with a
      Conventional-Commit-style message.
