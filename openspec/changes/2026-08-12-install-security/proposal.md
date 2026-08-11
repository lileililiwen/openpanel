## Why

`real-installs` made the Software Center actually install software
and made it work on every mainstream Linux family. It shipped one
missing piece: the install still ran as whatever user the panel
process is, with no privilege model. Operators either run the
panel as root (which is a footgun — a compromised panel means a
root shell) or the install fails with `Permission denied` on the
package tool's lock file. Neither is acceptable.

This change finishes the install story by giving the panel a
least-privilege install path that mirrors how Baota (宝塔),
cPanel, and Plesk ship in production: the panel process runs as
a normal user, package operations run through a tightly scoped
sudo allowlist, every install is audit-logged with its source and
digest, and a placeholder SHA-256 is treated as a fail-closed
condition for the live install path.

## What Changes

* **`PrivilegedCommand` wrapper.** Every package-tool invocation
  that needs root (apt-get, dnf, yum, zypper, pacman, apk) goes
  through a wrapper that prepends `sudo -n` when the panel is
  not running as root. Read-only commands (dpkg-query, rpm -qa,
  service smoke tests) short-circuit to the inner command. When
  the allowlist is missing, the wrapper fails with the exact
  `/etc/sudoers.d/openpanel` entry the operator must install.
* **Placeholder SHA-256 is fail-closed for live installs.** The
  recovery seed is allowed to ship entries whose upstream does
  not publish a hash (documented per recipe), but the live
  install path refuses those entries by default. Operators can
  set `OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=false` to
  opt back into the lenient behavior for an air-gapped
  recovery.
* **No user input flows into shell commands.** The install
  command for every `Family` is fixed (e.g. `apt-get -y install
  -- pkg`, `dnf -y install pkg`, `pacman --noconfirm -S --
  pkg`). The package name, the URL, and the destination path
  all come from the curated seed, not from the web form. The
  web form only carries a CSRF token and (for WordPress-style
  wizards) a domain and PHP version, which never touch the
  shell.
* **Audit trail.** Every install records an `AuditEvent` with
  the entry id, the source URL, the digest (or
  `digest_verified: false`), the bytes written, the host
  platform (`/etc/os-release` `ID`/`VERSION_ID`/arch), the
  actor, and the result. The storefront and the entry detail
  page surface the last successful install per entry.
* **No backdoor surface.** The Install button never asks the
  user for a URL, a script, or a path. The catalog is curated
  and signed; the panel never executes user-supplied shell; the
  sudo allowlist is fixed and listed in the failure message so
  the operator can audit it.

## Capabilities

### Modified Capabilities

- `software-center`: the existing capability gains the
  `PrivilegedCommand` wrapper, the placeholder-digest
  fail-closed policy, the audit trail, and the documented
  security model.

## Impact

Adds a new `PrivilegedCommand` struct in the `apt` module
(sibling of `TokioPackageCommand`); re-exports `PrivilegedCommand`
and `needs_privilege` from the software-center module root; adds
a `PLACEHOLDER_DIGEST_OPT_IN` config gate; threads
`audit` calls through every `install_artifact` and every
`HostPackageManager::apply`. No new external services. The
test surface grows by one service-level sudo test, one
placeholder-digest refusal test, and one audit-trail assertion.
