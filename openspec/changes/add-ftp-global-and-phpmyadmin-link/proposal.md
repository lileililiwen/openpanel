# Proposal: add-ftp-global-and-phpmyadmin-link

## WHY

Operators coming from Baota/cPanel expect two things that
OpenPanel lacks. First, FTP is reachable only per site at
`/sites/{id}/ftp` (`crates/openpanel-web/src/router.rs:400`):
there is no `/ftp` landing, no aggregate view, and the
capability registry (`capability_registry.rs`) carries only the
site-scoped `site-ftp` entry — so an operator with many sites
cannot see all FTP accounts at a glance. Second, the databases
list (`crates/openpanel-web/src/databases.rs:list_fragment`)
renders name/owner/charset/status with password + delete only;
Baota parity (password view/copy, quota, backup count, `.sql` /
`.zip` import, Local/Everyone/Specified-IP permission, Tools
repair/optimize/convert, phpMyAdmin deep-link, Sync-all /
Get-from-server adopt) is missing. The app layer already owns
the primitives this needs: `FtpService::list`, per-user RBAC,
`DatabasesService` with encrypted passwords, RemoteAccess ACL
enforcement, and the `admin-tool` SSO mint.

## WHAT

A global `GET /ftp` page aggregating FTP accounts across all
sites visible to the caller, each row linking back to its
site-scoped `/sites/{id}/ftp` page; plus enriched database
list/detail row actions (password view/copy one-time panel,
quota + backup count columns, import upload, permission editor,
Tools actions, phpMyAdmin SSO deep-link, Sync-all /
Get-from-server adopt flow). All actions reuse existing
services; handlers enforce RBAC + per-session CSRF.

## Capabilities

- **New:** `ftp` global aggregate list (`/ftp`) — delta in
  this change.
- **Modified:** `databases` — row-action enrichment + adopt
  flow (delta in this change).
- **Modified:** `capability-navigation` — `/ftp` registry
  entry + nav/route agreement (delta in this change).
- Reused unchanged: `sftp-jailed-shells` (no overlap),
  `web-ui` (shell, CSRF, styling tokens), identity/auth.

## Non-goals

- No new FTP daemon, transport, or credential semantics;
  `FtpService` and the supervisor are reused as-is.
- No phpMyAdmin vendoring or hosting; only an SSO deep-link
  via the existing `admin-tool` mint.
- No new backup engine; backup count is a read projection.
- No migration-wizard overlap (see sibling
  `add-migration-wizard-ui`) and no terminal overlap (see
  sibling `add-terminal-nav-and-saved-commands`).
- No fix for the dead `/hosts/{id}/terminal` link.
