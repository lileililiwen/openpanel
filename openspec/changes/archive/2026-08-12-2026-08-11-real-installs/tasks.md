## 1. TDD and Security Tests

- [x] 1.1 Unit-test `install_artifact` against the seed: adminer
      (single file) lands at
      `/var/lib/openpanel/webapps/adminer/4.8.1/adminer-4.8.1-en.php`
      with mode `0644`; the in-memory fetcher receives the exact
      URL the recipe pins; `digest_verified: false` is reported
      for the documented placeholder. —
      `install_artifact_downloads_and_places_adminer_under_webapps_root`
      in `crates/openpanel-app/tests/software_center.rs`.
- [x] 1.2 Service-test the host-aware package manager's
      command translation per family: every supported `Family`
      runs the expected program with the expected first argument
      and includes the package name. —
      `host_package_manager_translates_install_for_every_family`
      and `host_package_manager_translates_remove_for_every_family`.
- [x] 1.3 Service-test that a real package-tool failure (e.g.
      apt-get's `E: Unable to locate package apache2`) reaches
      the user as a `Package(detail)` error with the package name
      in the diagnostic. —
      `host_package_manager_surfaces_apt_get_failure_with_the_real_stderr`.
- [x] 1.4 Integration-test the web install flow: storefront
      shows the new Install button, posting to
      `/software/components/{id}/install` lands the file under
      the webapps root. Covers both the single-file case
      (adminer) and the tar.gz case (phpmyadmin) with a
      hand-built tar.gz fixture. —
      `web_install_button_for_adminer_downloads_and_places_the_php_file`
      and `web_install_button_for_a_tar_gz_web_entry_also_lands_on_disk`
      in `tests/integration/software_center.rs`.
- [x] 1.5 Proptest that `DnsName::new` rejects every
      non-`[a-zA-Z0-9.-]` character in a label. —
      `prop_invalid_dns_label_characters_never_survive` in
      `crates/openpanel-domain/tests/software_center.rs`.

## 2. Domain, Application, and Data Core

- [x] 2.1 Add `archive_type` values `tar.gz`, `tar.bz2`, `zip`,
      and `file` to `ArtifactPin::validate`; allow
      `archive_root` to be empty when `archive_type == "file"`. —
      `crates/openpanel-domain/src/software_center/recipe.rs`.
- [x] 2.2 Add `DnsName::new_with_underscore` for the
      underscore-prefix case; tighten `DnsName::new` to RFC 1035
      (letters/digits/hyphens). —
      `crates/openpanel-domain/src/dns/mod.rs`.
- [x] 2.3 Add the `ArtifactFetcher` trait with
      `ReqwestArtifactFetcher` (TLS-only, no redirect) and a
      `MemoryArtifactFetcher` test seam. —
      `crates/openpanel-app/src/software_center/artifact.rs`.
- [x] 2.4 Add `place_artifact` that dispatches on `archive_type`
      (tar.gz → extract; file → write verbatim). Honour the
      placeholder SHA-256 sentinel and report
      `digest_verified` on the result. —
      `crates/openpanel-app/src/software_center/artifact.rs`.
- [x] 2.5 Add `HostPackageManager` and the `Family` enum
      (Apt/Dnf/Yum/Pacman/Apk/Zypper) with `detect()` and
      per-family command translation. —
      `crates/openpanel-app/src/software_center/host_package_manager.rs`.
- [x] 2.6 Add `SoftwareCenterService::install_artifact(actor,
      role, entry_id)` that looks up the entry, fetches the
      pinned URL, and lands the artifact under
      `/var/lib/openpanel/webapps/{id}/{version}/`. —
      `crates/openpanel-app/src/software_center/mod.rs`.
- [x] 2.7 Carry `artifact: Option<ArtifactPin>` on
      `StorefrontVersion` so the install handler can read it
      from either the in-memory seed or the normalized SQLite
      store. —
      `crates/openpanel-app/src/software_center/store.rs`.
- [x] 2.8 Make `SoftwareCenterError::Package` carry a bounded
      diagnostic from the package tool; surface it through
      `software_center_error_message` and the API error mapper. —
      `crates/openpanel-app/src/software_center/mod.rs` and
      `crates/openpanel-web/src/software_center.rs`.

## 3. Storefront, Wizard, and Surface UI

- [x] 3.1 Render the Web entry Install button as a POST form to
      `/software/components/{id}/install` in both the storefront
      card and the entry detail page. —
      `crates/openpanel-web/src/software_center.rs`.
- [x] 3.2 Add the `install_artifact` route handler that calls
      the service and renders a typed success or error page. —
      `crates/openpanel-web/src/software_center.rs` and
      `crates/openpanel-web/src/router.rs`.
- [x] 3.3 Add the `class="button"` to every Confirm button on
      the preview / confirm / retry pages. —
      `crates/openpanel-web/src/software_center.rs`.
- [x] 3.4 Switch the production package manager to
      `HostPackageManager::detect` with `AptPackageManager` as
      the forced-debian fallback. —
      `crates/openpanel-app/src/software_center/module.rs`.
- [x] 3.5 Update the recovery seed: adminer uses a
      `file_artifact` (archive_type=`file`, archive_root=
      `adminer-4.8.1-en.php`). —
      `crates/openpanel-app/src/software_center/seed.rs`.

## 4. Test Support

- [x] 4.1 `TestServer` always uses an in-process
      `StagedArtifactFetcher` keyed by URL;
      `stage_artifact(url, bytes)` is the per-test hook.
      Anything not staged is rejected with
      `SoftwareCenterError::Package`, so a test that forgets to
      stage the bytes fails loudly instead of hanging on a real
      network call. —
      `crates/openpanel-test-support/src/server.rs`.
- [x] 4.2 Expose `server.fetched_artifacts()` and
      `server.webapps_root()` for end-to-end assertions. —
      `crates/openpanel-test-support/src/server.rs`.

## 5. Validation and Delivery

- [x] 5.1 `cargo test --workspace` is green: 121 integration
      tests pass, every unit test passes, no proptest
      regressions. —
      `cargo test --workspace`.
- [x] 5.2 `make fmt` and `make clippy` are clean. —
      `make fmt` and `make clippy`.
- [x] 5.3 Archive via `openspec archive`; commit with a
      Conventional-Commit-style message. —
      `git commit` and the `archive` step.
