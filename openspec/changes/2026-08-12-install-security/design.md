# Design

## Least-privilege install path

The current production path runs the panel as whatever user
started the binary. If that user is root, the install works; if
not, every package-tool invocation fails with `Permission
denied` on `/var/lib/dpkg/lock-frontend` (Debian) or the
rpm/dnf equivalent. The fix is the pattern used by every
production server panel: the panel process is a normal user, and
package operations escalate through `sudo -n` over a fixed,
auditable allowlist.

`PrivilegedCommand` is a new `PackageCommand` implementation in
`crates/openpanel-app/src/software_center/apt.rs` that wraps an
inner `PackageCommand` and:

1. Inspects the program name. If it is not in
   [`needs_privilege`], short-circuits to the inner command.
2. If the program needs privilege and the panel is running as
   root (`uid == 0`), short-circuits to the inner command.
3. Otherwise, prepends `sudo -n` to the program name and calls
   the inner command with the wrapped argument list.
4. On non-zero exit, returns
   `SoftwareCenterError::Package("…failed: …. The panel is not
   running as root and `sudo -n` (non-interactive) rejected the
   request. Add the following line to
   /etc/sudoers.d/openpanel on this host and reload sudo
   (visudo -c && systemctl restart sudo):\n\nopenpanel
   ALL=(root) NOPASSWD: /usr/bin/apt-get, /usr/bin/dnf,
   /usr/bin/yum, /usr/bin/zypper, /usr/bin/pacman,
   /sbin/apk\n\nOr run the panel as the root user (not
   recommended).")`.

`needs_privilege` is a fixed allowlist:
`/usr/bin/apt-get`, `/usr/bin/dnf`, `/usr/bin/yum`,
`/usr/bin/zypper`, `/usr/bin/pacman`, `/sbin/apk`. Read-only
commands (`/usr/bin/dpkg-query`, `/usr/bin/rpm`, nginx
`-t`, mysqladmin `ping`, etc.) are NOT in the allowlist and
short-circuit to the inner command — they work fine as the
panel user.

The wrapper detects root by reading
`/proc/self/status` and parsing the `Uid:` line, not by calling
`getuid()`. The workspace lints forbid `unsafe` blocks, and
`/proc/self/status` is the only portable, unprivileged way to
get the real UID without linking `libc` directly.

The production constructor in `module.rs` wraps the
`TokioPackageCommand` with `PrivilegedCommand` for every
privileged binary. The `FakePackageManager` and the test
environment skip the wrapper (they use mockall-generated
commands that record invocations instead of running real
binaries).

## Fail-closed digest policy

The `real-installs` change allowed the placeholder SHA-256
(all zeros) to skip the digest check, on the grounds that the
recovery seed needs to ship entries whose upstream does not
publish a hash. The problem: an operator who only has the
recovery seed cannot tell which installs were verified, and the
test seam can be used to push a known-bad artifact through the
install path because the digest check is bypassed.

This change introduces a `require_verified_digests` gate. The
gate is read at startup from
`OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS` (default
`true`). When the gate is on, `place_artifact` returns
`SoftwareCenterError::Invalid("digest placeholder is not allowed
when OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS is true")`
for any entry whose `pin.sha256` is the placeholder sentinel.
The seed for entries whose upstream does not publish a hash
becomes "view-only" — the entry shows up in the catalog, the
Install button is disabled, and the entry detail page explains
that the operator must run `software refresh` against a remote
catalog that pins a real digest before installing.

The gate is a startup-time check, not a per-install check, so
the cost is one env-var read at boot.

## Audit trail

`SoftwareCenterService` already records an `AuditEvent` for
every install path. This change extends the recorded fields:

- The host platform (`/etc/os-release` `ID` +
  `VERSION_ID` + arch), captured at install time so a
  post-mortem can tell which host the install ran against.
- The source URL for Web entries.
- The recipe digest and `digest_verified` for Web entries.
- The package list for System entries.
- The actor (the user who clicked Install).
- The result (`installed`, `rolled_back`, `failed: <detail>`).

The storefront and the entry detail page read the last
`installed` event per entry and surface it in a "Last install"
badge so the operator can see who installed what and when
without having to query the audit log directly.

## No user input in shell

The install command for every `Family` is hard-coded:

| Family | Install | Remove | Update |
|---|---|---|---|
| Apt | `/usr/bin/apt-get -y install -- pkg` | `/usr/bin/apt-get -y remove -- pkg` | `/usr/bin/apt-get -y install --only-upgrade pkg` |
| Dnf | `/usr/bin/dnf -y install pkg` | `/usr/bin/dnf -y remove pkg` | `/usr/bin/dnf -y update pkg` |
| Yum | `/usr/bin/yum -y install pkg` | `/usr/bin/yum -y remove pkg` | `/usr/bin/yum -y update pkg` |
| Pacman | `/usr/bin/pacman --noconfirm -S -- pkg` | `/usr/bin/pacman --noconfirm -R -- pkg` | `/usr/bin/pacman --noconfirm -S -u pkg` |
| Apk | `/sbin/apk add pkg` | `/sbin/apk del pkg` | `/sbin/apk upgrade pkg` |
| Zypper | `/usr/bin/zypper --non-interactive install -- pkg` | `/usr/bin/zypper --non-interactive remove -- pkg` | `/usr/bin/zypper --non-interactive update pkg` |

The package name comes from `entry.versions[*].packages`, which
is set by the seed. The web form carries only `_csrf` (and
domain/php_version/locale for the WordPress-style wizard,
which goes through a separate path). No part of the install
command is interpolated from user input.

For Web entries, `install_artifact` reads the URL and digest
from the seed; the fetcher passes only those two values to
`reqwest::Client::get`. The web form carries only `_csrf`. The
destination path is `webapps_root/<version>/<filename>`,
derived from the seed's `archive_type` and `archive_root`, not
from user input.

The Install button on the storefront and the entry detail
page has no `name` or `script` field. The catalog never
prompts the user for a URL, a path, or a shell command. The
only user input that affects the install is "which entry did
you click", which is itself a curated id from the seed.
