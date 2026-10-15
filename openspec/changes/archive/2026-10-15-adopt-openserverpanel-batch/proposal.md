# Proposal: adopt-openserverpanel-batch

## Why

A capability comparison against OpenServerPanel (OSPanel, the
portable Windows dev-stack panel) surfaced four gaps that hosting
operators keep asking for and that our own tracking documents already
flag: co-installable software versions (`docs/competitive-gap-analysis.md`
neighbour concerns), live certificate issuance (the
`ssl-production-lifecycle` spec declares `RustlsAcmeClient::issue` a
critical production gap), WebDAV (listed as an unspec'd gap in
`docs/competitive-gap-analysis.md` and `HANDOFF.md`), and a staging
mail sandbox (OSPanel ships Mailpit/SMTP4dev; we have `site-staging`
but outbound test mail still escapes to the real MTA).

## What Changes

- **Multi-version Software Center installs**: the storefront already
  models `versions` per catalog entry, but installed state keys
  `software_components` by component only and the install APIs take no
  version. Add version-targeted install/preview/uninstall, co-existence
  of multiple installed versions per component, and per-version update
  status.
- **Live ACME HTTP-01 issuance**: replace the `RustlsAcmeClient::issue`
  skeleton with complete rustls-acme wiring (directory discovery,
  account/order, HTTP-01 via the existing challenge server,
  finalization, cert retrieval) behind the existing staging-default /
  production-opt-in gating. Fail-closed persistence unchanged.
- **WebDAV jailed file access (new capability `webdav-file-access`)**:
  a per-site WebDAV account surface reusing the FTP account shape
  (chroot home, read-only flag, TLS-mode awareness), served over the
  existing axum stack with the core read/write/lock method set
  (OPTIONS, PROPFIND, GET, HEAD, PUT, DELETE, MKCOL, MOVE, COPY,
  PROPPATCH no-content).
- **Staging mail capture (new capability `staging-mail-capture`)**:
  when a site's staging slot enables capture, outbound messages from
  that site are stored in a SQLite-backed capture inbox instead of the
  MTA; owners/admins can list, read and purge captured messages.

## Capabilities

### New Capabilities
- `webdav-file-access`: jailed WebDAV accounts per site, storage
  backend reuse, method-level authorization, and API/web surfaces.
- `staging-mail-capture`: per-staging-slot capture toggle, capture
  sink semantics, inbox visibility, and retention/purge.

### Modified Capabilities
- `software-center`: ADD versioned-install requirements (co-installable
  versions, version-targeted operations, per-version status).
- `ssl-production-lifecycle`: MODIFY "Complete HTTP-01 Issuance" to
  close the skeleton gap — live wiring implemented, no residual stub
  error path.
- `site-staging`: ADD capture-mail requirement hooking slot lifecycle
  (capture follows slot destruction).

## Impact

- Crates: `openpanel-domain` (new `webdav`, `mail_capture` models;
  software-center VOs), `openpanel-app` (`software_center`, `ssl`,
  `webdav`, `mail_capture`, `site_staging` modules + migrations),
  `openpanel-api` (routes), `openpanel-web` (pages), `openpanel-cli`
  (subcommands), `openpanel-core` (error variants if needed).
- DB: new migrations for `software_components` version keying,
  `webdav_accounts`, `mail_capture_messages`.
- Dependencies: none new (rustls-acme, libunftp-shaped auth reuse,
  lettre already vendored).
- No breaking API changes: existing endpoints gain optional fields.

## Non-goals

- No OSPanel-style local-dev/portable mode, Windows support, or bundled
  CLI tool arsenal.
- No catalog expansion (MongoDB/RabbitMQ recipes) in this batch.
- WebDAV excludes locking (`LOCK`), ACL, and range streaming beyond
  axum defaults; PROPPATCH returns empty 207.
- Mail capture is staging-site scoped only; production mail flow and
  the notification SMTP channel are untouched.
- Per-site PHP/Node runtime *switching UX* stays in existing specs;
  this batch only makes co-installation possible.
