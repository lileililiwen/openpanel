# Design

## One-click install for Web entries

The existing `preview_deployment` / `execute_deployment` wizard is a
two-step flow (preview, confirm) that creates a site, a database, and
installs the artifact. It is only wired for wordpress and drupal. The
Adminer case (and every other Web entry) is a single PHP file that
needs to land at a managed path; the wizard is overkill and a
one-shot install is what the operator actually wants.

`SoftwareCenterService::install_artifact(actor, role, entry_id)` is a
new one-step method that:

1. Reads the entry from `self.store.get_entry(entry_id)`.
2. Rejects entries whose `kind` is not `Web`.
3. Picks the latest version (or the first one if `is_latest` is
   missing).
4. Reads the version's `ArtifactPin` (URL + digest + archive_type +
   archive_root).
5. Calls `self.artifact_fetcher.fetch(url)` to stream the bytes
   through the new `ArtifactFetcher` trait.
6. Calls `place_artifact(&pin, &bytes, &self.webapps_root)` which
   dispatches on `archive_type`:
   - `tar.gz` — `SafeArtifactInstaller::place_tar_gz` extracts into
     `webapps_root/<version>/` after verifying the digest (or
     skipping the check for documented placeholders).
   - `file` — `SafeArtifactInstaller::place_single_file` writes the
     bytes to `webapps_root/<version>/<filename>` and `chmod 0644`s
     the file.
7. Records an `artifact_installed` audit event and returns an
   `ArtifactInstallResult` with the entry id, version, destination
   path, filename (for `file` archives), bytes written, and whether
   the digest was verified.

The web route is `POST /software/components/{id}/install`. The
storefront and the entry detail page both render the Install button
as a POST form with a single hidden `_csrf` field. There is no
preview step.

## Host-aware package manager

The previous `AptPackageManager::discover` hardcoded a check for
"ubuntu 22.04/24.04, debian 12" and otherwise returned
`SoftwareCenterError::Invalid`. Every other Linux family was
unsupported.

`HostPackageManager` is a new struct that holds one detected
`Family` plus the path to its primary binary
(`/usr/bin/apt-get`, `/usr/bin/dnf`, …). `Family` is an enum with
six variants. `HostPackageManager::detect(command)` probes the host:

1. `/usr/bin/apt-get` → `Family::Apt`
2. `/usr/bin/dnf` → `Family::Dnf` (probed before yum so Fedora 41+
   doesn't fall through)
3. `/usr/bin/yum` → `Family::Yum`
4. `/usr/bin/zypper` → `Family::Zypper`
5. `/usr/bin/pacman` → `Family::Pacman`
6. `/sbin/apk` → `Family::Apk`

If none are present, `detect` returns
`SoftwareCenterError::Package` with a message that names every
binary it looked for.

Each `Family` knows its own install, remove, and list argument
shapes. `apply` dispatches to the right shape per family and uses
`Update` for the `Update` plan action (`install --only-upgrade` for
apt, `update` for dnf/yum/zypper, `install -u` for pacman,
`upgrade` for apk). `discover` reads `/etc/os-release` (no longer
restricted to two distros), builds a `SupportedPlatform` from the
detected `ID` and `VERSION_ID`, and runs the family-specific list
command to enumerate installed packages.

`AptPackageManager` is preserved as a forced-debian fallback for
callers that want to skip detection. The production constructor in
`module.rs` prefers `HostPackageManager::detect` and falls back to
`AptPackageManager` only when the probe fails.

## URL validation, digest policy, and the placeholder escape hatch

`ArtifactDownloader::download` used to validate the URL against a
two-host allowlist. The recovery seed pins URLs on
`files.phpmyadmin.net`, `github.com`, `download.nextcloud.com`,
`builds.matomo.org`, etc.; an allowlist makes most of the catalog
uninstallable. The new `validate_artifact_url` only refuses
non-HTTP(S), userinfo, port, query, and fragment. The rest of the
URL hygiene is delegated to the recipe's pinned digest.

The placeholder SHA-256 (`"00…00"`) is the documented "no digest
supplied" sentinel for entries whose upstream does not publish a
hash. `place_artifact` and `ArtifactDownloader::download` skip the
digest check when the recipe's `sha256` equals this placeholder;
the install result reports `digest_verified: false` so the operator
can audit which installs were verified.

## Error pages

`SoftwareCenterError::Package` carries a bounded, redacted
diagnostic instead of a unit variant. The web shell's
`software_center_error_message` maps every `SoftwareCenterError`
variant to a user-readable explanation; for `Package(detail)` it
appends the detail so the operator sees the real reason an install
failed (`E: Unable to locate package apache2`, `Permission denied`,
etc.). The API layer collapses `Package` to a 500 with the message
attached.

## DNS labels

`DnsName::new` is now strict: labels accept only ASCII letters,
digits, and hyphens. The previous version accepted `_` as a label
character, which violated RFC 1035 and the
`prop_invalid_dns_label_characters_never_survive` proptest. A new
`DnsName::new_with_underscore` is used by `create_record` for TXT
records (the ACME `_acme-challenge`, DKIM `_dmarc`, and DMARC
underscore-prefix cases).
