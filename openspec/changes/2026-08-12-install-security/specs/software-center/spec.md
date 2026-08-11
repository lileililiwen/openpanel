# Spec delta: software-center

## ADDED Requirements

### Requirement: Least-privilege install path

The Software Center SHALL NOT require the panel process to run as
root. When the panel is not root, every package-tool invocation
that requires privilege SHALL be wrapped in `sudo -n` (non-
interactive) over a fixed allowlist. The allowlist is:

```
openpanel ALL=(root) NOPASSWD: /usr/bin/apt-get, /usr/bin/dnf, /usr/bin/yum, /usr/bin/zypper, /usr/bin/pacman, /sbin/apk
```

Read-only commands (dpkg-query, rpm, nginx `-t`, mysqladmin
`ping`, etc.) SHALL NOT be wrapped in `sudo`. The wrapper
SHALL detect root by reading `/proc/self/status` and parsing
the `Uid:` line, NOT by calling `getuid()`. The wrapper SHALL
be `unsafe`-free (the workspace lints forbid `unsafe` blocks).

When `sudo -n` rejects the request, the wrapper SHALL return
`SoftwareCenterError::Package` whose detail includes the
exact `/etc/sudoers.d/openpanel` entry the operator must
install, the `visudo -c && systemctl restart sudo` reload
command, and a pointer to this spec.

#### Scenario: Panel is not root, sudo allowlist is missing

- **WHEN** the panel process has `uid != 0` and the operator
  has not installed the allowlist in `/etc/sudoers.d/openpanel`
- **THEN** `sudo -n /usr/bin/apt-get -y install -- apache2`
  exits non-zero with `sudo: a password is required`, the
  wrapper returns `Package("… failed: …. The panel is not
  running as root and `sudo -n` (non-interactive) rejected the
  request. Add the following line to
  /etc/sudoers.d/openpanel on this host and reload sudo
  (visudo -c && systemctl restart sudo):\n\nopenpanel
  ALL=(root) NOPASSWD: /usr/bin/apt-get, /usr/bin/dnf,
  /usr/bin/yum, /usr/bin/zypper, /usr/bin/pacman,
  /sbin/apk\n\nOr run the panel as the root user (not
  recommended).")`, the web shell renders a typed error
  page with the same text, and the install is recorded as
  `failed` in the audit log with the sudo rejection message.

#### Scenario: Panel is not root, sudo allowlist is installed

- **WHEN** the panel process has `uid != 0` and the operator
  has installed the allowlist in `/etc/sudoers.d/openpanel`
- **THEN** `sudo -n /usr/bin/apt-get -y install -- apache2`
  exits zero, the install succeeds, the audit log records
  `installed` with the actor, the entry id, and the host
  platform, and the storefront surfaces the success.

#### Scenario: Panel is root

- **WHEN** the panel process has `uid == 0`
- **THEN** the wrapper short-circuits to the inner command;
  no `sudo` is invoked, the install runs as root directly, and
  the audit log records the install with `actor=root` and
  the host platform.

#### Scenario: Read-only command (dpkg-query) does not escalate

- **WHEN** `HostPackageManager::discover` runs
  `/usr/bin/dpkg-query -W -f=…` on a non-root panel
- **THEN** the wrapper short-circuits to the inner command
  (no `sudo`), the command runs as the panel user, and
  the snapshot succeeds without prompting for a password.

### Requirement: Placeholder SHA-256 is fail-closed for live installs

The recovery seed is allowed to ship entries whose upstream
does not publish a hash (the `sha256` field equals the
all-zeros placeholder sentinel). For the live install path
(`install_artifact` and `HostPackageManager::apply`), the
Software Center SHALL refuse any install whose recipe
carries the placeholder digest, UNLESS the operator has set
`OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false` at
startup.

When the gate is on (the default), `place_artifact` SHALL
return `SoftwareCenterError::Invalid("digest placeholder is
not allowed when OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS
is true")` for any Web entry whose `pin.sha256` is the
placeholder sentinel. The Install button for that entry
SHALL be disabled in the storefront and the entry detail
page SHALL explain that the operator must run
`software refresh` against a remote catalog that pins a real
digest before installing.

The gate is a startup-time check (one env-var read at
boot), not a per-install check.

#### Scenario: adminer seed ships a placeholder digest

- **WHEN** the recovery seed for adminer carries
  `sha256: "0000…0000"` and the gate is on
- **THEN** `install_artifact` for adminer returns
  `Invalid("digest placeholder is not allowed when
  OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS is true")`,
  the storefront renders the Install button as disabled,
  and the entry detail page shows the message above the
  Versions tab.

#### Scenario: opt-in via env var for air-gapped recovery

- **WHEN** the operator sets
  `OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false`
- **THEN** the install proceeds with the placeholder skipped
  (existing behavior, `digest_verified: false` is reported)
  so the operator can install from the embedded recovery
  seed on a host with no network access to a remote
  catalog.

### Requirement: No user input in shell commands

The install command for every `Family` SHALL be a fixed
string. The package name SHALL come from
`entry.versions[*].packages` (set by the seed), not from
the web form. The web form SHALL carry only `_csrf` (and
domain/php_version/locale for the WordPress-style wizard,
which goes through a separate path). The Install button
SHALL NOT have a `name`, `script`, or `path` field.

For Web entries, the URL and digest SHALL come from
`pin.url` and `pin.sha256` (set by the seed), not from the
web form. The destination path SHALL be
`webapps_root/<version>/<filename>`, derived from the
seed's `archive_type` and `archive_root`, not from user input.

#### Scenario: Click Install on adminer

- **WHEN** an Owner clicks Install on adminer
- **THEN** the panel POSTs to
  `/software/components/adminer/install` with only a
  `_csrf` field. The service looks up the entry, reads
  the URL and digest from the seed, downloads the file,
  verifies the digest (or skips for a documented
  placeholder under the opt-in env var), and lands the
  file at
  `/var/lib/openpanel/webapps/adminer/4.8.1/adminer-4.8.1-en.php`.
  No part of the install command, the URL, the digest, or
  the destination path comes from the web form.

### Requirement: Audit trail for every install

Every install (Web or System) SHALL record an `AuditEvent`
with the following fields:

- The entry id and display name.
- The actor (the user who clicked Install).
- For System entries: the package list and the
  `Family` used for the install.
- For Web entries: the source URL, the recipe digest,
  and `digest_verified`.
- The host platform (`/etc/os-release` `ID` +
  `VERSION_ID` + arch), captured at install time.
- The result: `installed`, `failed: <detail>`, or
  `rolled_back`.

The storefront and the entry detail page SHALL read the
last `installed` event per entry and surface it in a "Last
install" badge.

#### Scenario: Owner installs nginx

- **WHEN** an Owner installs the `nginx` System entry on
  an Ubuntu 24.04 host
- **THEN** the audit log records
  `actor=alice, entry=nginx, family=Apt,
  packages=[nginx], platform=ubuntu 24.04 x86_64,
  result=installed`. The entry detail page shows
  "Last install: alice · ubuntu 24.04 · succeeded".

#### Scenario: Owner installs adminer

- **WHEN** an Owner installs the `adminer` Web entry with
  the verified-digest gate off
- **THEN** the audit log records
  `actor=alice, entry=adminer, source_url=…,
  digest=0000…0000, digest_verified=false,
  platform=ubuntu 24.04 x86_64, result=installed`. The
  storefront shows the adminer card with "Last install:
  alice · digest unverified".

## REMOVED Requirements

None.

## RENAMED Requirements

None.

## MODIFIED Requirements

- `software-center::Host-aware package manager` (added in
  2026-08-11): the production constructor in `module.rs`
  SHALL wrap the `TokioPackageCommand` with
  `PrivilegedCommand` for every privileged binary, NOT
  call it directly. The existing
  `HostPackageManager::detect` constructor stays
  unchanged.

- `software-center::Generic artifact placement` (added in
  2026-08-11): the placeholder SHA-256 escape hatch SHALL
  be gated by `OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS`.
  The default is `true` (fail closed).
