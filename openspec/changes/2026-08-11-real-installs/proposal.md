## Why

The previous `upgrade-software-center` change shipped a curated Software
Center that covered Debian/Ubuntu only and let every Web entry show a
generic Install button that always 422'd, because the only working
deployment path (the `preview_deployment` wizard) was hardcoded to
wordpress/drupal. Operators running any other distribution hit a
hardcoded `apt-get` that doesn't exist on their host, and an Install
button that didn't actually install anything on the system.

This change makes the Software Center actually install software, and
makes it install on every mainstream Linux family.

## What Changes

* **One-click install for any Web entry.** The Install button on a Web
  entry now POSTs to `/software/components/{id}/install`. The handler
  calls `SoftwareCenterService::install_artifact`, which looks up
  the entry, fetches the pinned URL, and lands the file or extracted
  archive under `/var/lib/openpanel/webapps/{id}/{version}/`. No
  preview, no confirmation token, no wizard. Every entry with a
  pinned `ArtifactPin` works.
* **Generic artifact placement.** `place_artifact` dispatches on the
  `archive_type` in the recipe: `tar.gz` is extracted into a versioned
  directory; `tar.bz2` and `zip` are queued for the same path; a
  single-file `file` artifact is written verbatim under the same
  directory. The recovery seed for adminer now uses `archive_type:
  "file"` so the single PHP file lands at
  `/var/lib/openpanel/webapps/adminer/4.8.1/adminer-4.8.1-en.php`.
* **Host-aware package manager.** `AptPackageManager` is no longer the
  default. A new `HostPackageManager` probes the running host for
  whichever native package tool is installed — `apt-get` (Debian,
  Ubuntu, derivatives), `dnf` (Fedora 41+, RHEL 9+), `yum` (RHEL 8,
  CentOS Stream, Rocky, Alma), `zypper` (openSUSE, SUSE Linux
  Enterprise), `pacman` (Arch, Manjaro, Endeavour), `apk` (Alpine) —
  and dispatches install/remove/snapshot to the right command. The
  probe order is dnf-before-yum so Fedora 41+ doesn't fall through to
  the yum shim, then zypper, then pacman and apk. `AptPackageManager`
  is preserved as a forced-debian fallback.
* **Strict URL policy replaced with HTTP sanity checks.** The old
  `validate_artifact_url` allowlisted only `wordpress.org` and
  `ftp.drupal.org`, which meant the recovery seed for phpmyadmin,
  joomla, ghost, typecho, nextcloud, matomo, bookstack, etc. could
  never install. The new check only refuses non-HTTP(S), userinfo,
  port, query, or fragment; the rest of the URL hygiene (allowlist
  per recipe) is delegated to the recipe's pinned digest.
* **Placeholder SHA-256 is allowed but visible.** The recovery seed
  ships entries whose upstream does not publish a hash
  (documented per recipe). For these entries, `place_artifact` skips
  the digest check and the result records `digest_verified: false`
  so the operator can audit which installs were verified.
* **Better error pages.** `execute` and `execute_deployment` now
  render a real success or typed-error page through the authed shell
  instead of returning 200-with-no-body or a blanket 422. A new
  `software_center_error_message` helper maps every
  `SoftwareCenterError` variant to a user-readable explanation;
  `SoftwareCenterError::Package` now carries the bounded diagnostic
  from the package tool (apt-get stderr, dnf stderr, etc.) so the
  user sees the real reason an install failed.
* **All Confirm buttons styled.** The four "Confirm" buttons
  (`preview`, `preview_component_action`, `preview_deployment`,
  `retry`) carry the `button` class so they are styled by the shell.
* **`DnsName` is strict by default.** `DnsName::new` no longer
  accepts `_` in label characters (it only accepts
  letters/digits/hyphens per RFC 1035). A new
  `DnsName::new_with_underscore` is used for the ACME/DKIM/DMARC
  case via `create_record` for TXT records. The proptest that pinned
  the strict contract now passes.
* **Test server always uses an in-process artifact fetcher.**
  `TestServer::stage_artifact(url, bytes)` is the per-test hook that
  replaces the network; without staging, the fetcher fails loudly
  with `SoftwareCenterError::Package` instead of hanging on a real
  HTTP call.

## Capabilities

### Modified Capabilities

- `software-center`: the existing capability gains the host-aware
  package manager, the one-click Web install path, the generic
  artifact placement, the HTTP-sanity-check URL validator, the
  placeholder-digest escape hatch, the better error pages, and the
  styling of every confirm button.

## Impact

Adds a new `host_package_manager` module, a new
`install_artifact` service method, a new `POST
/software/components/{id}/install` route, an `artifact_type` field
on `StorefrontVersion` and a `manifest_json` round-trip through the
SQLite store. The `DnsName` invariant tightens. The recovery seed
gets a new `file_artifact` helper and adminer's recipe switches to
it. No new external services; the catalog remains embedded with a
remote signed override. Every existing test still passes; the new
test surface is a service-level install_artifact test, an end-to-end
web install test for adminer and phpmyadmin, and a per-family
command-translation test.
