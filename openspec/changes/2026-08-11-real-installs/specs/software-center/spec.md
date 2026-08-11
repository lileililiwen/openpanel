# Spec delta: software-center

## ADDED Requirements

### Requirement: One-click install for Web entries

Every Web entry that carries a pinned `ArtifactPin` SHALL install
in one step. The user clicks Install on the storefront or the
entry detail page; the panel downloads the pinned URL, verifies
the digest (or skips the check for a documented placeholder
SHA-256), and lands the file or extracted archive at
`/var/lib/openpanel/webapps/{id}/{version}/`. There SHALL be no
preview, no confirmation token, and no wizard step. The handler
SHALL be `POST /software/components/{id}/install`.

#### Scenario: Adminer single-file install

- **WHEN** an Owner clicks Install on the Adminer entry
- **THEN** the panel POSTs to
  `/software/components/adminer/install`, the service downloads
  `https://github.com/vrana/adminer/releases/download/v4.8.1/adminer-4.8.1-en.php`,
  and the file lands at
  `/var/lib/openpanel/webapps/adminer/4.8.1/adminer-4.8.1-en.php`
  with mode `0644`. The success page reports the destination path,
  the bytes written, the archive type (`file`), and whether the
  digest was verified (false for the documented placeholder).

#### Scenario: phpMyAdmin tar.gz install

- **WHEN** an Owner clicks Install on the phpMyAdmin entry
- **THEN** the panel downloads
  `https://files.phpmyadmin.net/phpMyAdmin/5.2.2/phpMyAdmin-5.2.2-all-languages.tar.gz`,
  verifies the digest (or skips the check for the documented
  placeholder), and extracts the archive into
  `/var/lib/openpanel/webapps/phpmyadmin/5.2.2/`. The success
  page reports the destination directory and the bytes written.

#### Scenario: System entry must use the package manager, not the artifact path

- **WHEN** an Owner clicks Install on a System entry (nginx,
  php-8.3, mysql, etc.)
- **THEN** the Install button is not wired to
  `/software/components/{id}/install`; the install is driven by
  the host package manager via the existing
  `preview_install` / `execute` flow.

### Requirement: Host-aware package manager

The Software Center SHALL detect the host's native package tool
at startup and dispatch every `apply`, `rollback`, and `discover`
call to that tool. The detector SHALL look for the following
binaries in this order:

1. `/usr/bin/apt-get` — Debian, Ubuntu, and derivatives
2. `/usr/bin/dnf` — Fedora 22+, RHEL 9+, Nobara, Bazzite
3. `/usr/bin/yum` — RHEL 8, CentOS Stream 8, Rocky 8, Alma 8
4. `/usr/bin/zypper` — openSUSE, SUSE Linux Enterprise
5. `/usr/bin/pacman` — Arch, Manjaro, EndeavourOS
6. `/sbin/apk` — Alpine

If none of these are present, the detector SHALL return
`SoftwareCenterError::Package` with a message that names every
binary it looked for.

The dispatcher SHALL translate `PlanAction::Install` to the
family's install command, `PlanAction::Remove` to the family's
remove command, and `PlanAction::Update` to the family's update
command. The translation SHALL be tested per family.

#### Scenario: apt-get is detected on Debian

- **WHEN** the panel starts on a Debian 12 host
- **THEN** the detector finds `/usr/bin/apt-get` and binds the
  service to `Family::Apt`. A `preview_install` for the
  `apache2` entry runs `/usr/bin/apt-get -y install -- apache2`
  inside the package transaction lock.

#### Scenario: dnf is detected on Fedora 41

- **WHEN** the panel starts on a Fedora 41 host
- **THEN** the detector finds `/usr/bin/dnf` (probed before yum)
  and binds the service to `Family::Dnf`. A `preview_install`
  for the `nginx` entry runs `/usr/bin/dnf -y install nginx`.

#### Scenario: pacman is detected on Arch

- **WHEN** the panel starts on an Arch Linux host
- **THEN** the detector finds `/usr/bin/pacman` and binds the
  service to `Family::Pacman`. A `preview_install` for the
  `nginx` entry runs
  `/usr/bin/pacman --noconfirm -S -- nginx`.

#### Scenario: unknown distro fails closed

- **WHEN** the panel starts on a host with no recognised
  package tool
- **THEN** the detector returns
  `SoftwareCenterError::Package("no supported package manager
  found on this host (looked for apt-get, dnf, yum, zypper,
  pacman, apk)")` and the service is left unconfigured. The
  fallback constructor in `module.rs` switches to
  `AptPackageManager` (which will then fail on `discover` if
  apt-get is also missing) so the existing error reporting path
  still applies.

### Requirement: Generic artifact placement

The artifact installer SHALL dispatch on the recipe's
`archive_type`. The supported values are:

- `tar.gz` — extract into `webapps_root/<version>/`, with the
  recipe's `archive_root` as the expected top-level directory
- `tar.bz2` — same as `tar.gz`
- `zip` — same as `tar.gz`
- `file` — write the bytes verbatim to
  `webapps_root/<version>/<filename>`, where `filename` is the
  recipe's `archive_root` or, if that is empty, the basename of
  the URL

`place_artifact` SHALL refuse `file` artifacts whose
`archive_root` contains a path separator or whose derived filename
is empty. The placeholder SHA-256 (all zeros) SHALL be allowed
only as a documented "no digest supplied" sentinel; the install
result SHALL record `digest_verified: false` when the check is
skipped.

### Requirement: Better error pages for the install flow

`SoftwareCenterError::Package` SHALL carry a bounded, redacted
diagnostic from the package tool (apt-get stderr, dnf stderr,
etc.). The web shell's `software_center_error_message` SHALL
append that diagnostic to the user-visible message so the
operator can see the real reason an install failed (e.g.
`E: Unable to locate package apache2`, `Permission denied`).

The `execute` and `execute_deployment` handlers SHALL render a
real success or typed-error page through the authed shell
instead of returning 200-with-no-body or a blanket 422. Every
Confirm button on those pages (`preview`,
`preview_component_action`, `preview_deployment`, `retry`) SHALL
carry the `button` class so it is styled by the shell.

### Requirement: DNS labels are strict by default

`DnsName::new` SHALL accept only ASCII letters, digits, and
hyphens in label characters (RFC 1035). A separate
`DnsName::new_with_underscore` SHALL exist for the
underscore-prefix case used by ACME challenges
(`_acme-challenge`), DKIM keys, DMARC records, and other TXT
records that require `_`. `create_record` SHALL use the relaxed
constructor for TXT records and the strict constructor for
hostnames resolved by A, AAAA, CNAME, MX, NS, and SRV.

The proptest `prop_invalid_dns_label_characters_never_survive`
SHALL pass for every distribution: any non-alphanumeric,
non-`-`, non-`.` character in a label produces
`DnsName::new(...).is_err()`.

## REMOVED Requirements

None.

## RENAMED Requirements

None.

## MODIFIED Requirements

None.
