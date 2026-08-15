# Add WordPress toolkit

## Why

The `web-application-installer` change provides a generic one-click
install for any web app, but WordPress operators need WP-specific
operations that a generic installer does not cover. Plesk WordPress
Toolkit and cPanel WP Manager offer staging, cloning, core/plugin/theme
update management with rollback, a security scan (file permissions,
versions, known-vuln checks), and a caching toggle. Without a dedicated
toolkit, OpenPanel users manage WordPress manually over SSH, losing
safe staging, atomic update rollback, and a security baseline. This
change adds a `wordpress-toolkit` bounded context that operates on
sites where WordPress is installed.

## What Changes

- New bounded context `wordpress-toolkit` carrying the `WpSite`
  aggregate and `WpToolkitService`, operating only on sites flagged as
  WordPress.
- One-click staging of a WP site (reusing `site-staging`); clone of a WP
  site (reusing `add-site-clone-and-template-export`).
- Core / plugin / theme update management with rollback to the prior
  version on failure.
- Security scan: file permission check, version check against the
  latest, and a known-vulnerability check against a local CVE feed.
- Cache toggle (object/page cache on/off) written to the WP config.
- New endpoints: `/sites/{id}/wordpress/staging`,
  `/sites/{id}/wordpress/clone`, `/sites/{id}/wordpress/updates`,
  `/sites/{id}/wordpress/security`,
  `/sites/{id}/wordpress/cache`.

## Capabilities

### New Capabilities

- `wordpress-toolkit`: WP-aware staging, clone, update management with
  rollback, security scan, and cache toggle for installed WordPress
  sites.

## Impact

- Domain: `WpSite`, `WpUpdateSet`, `WpUpdateResult`, `WpSecurityReport`,
  `WpCacheMode`.
- App: `WpToolkitService`, `WpScanner`, `WpUpdater`, `WpCacheLayer`.
- API/CLI/web: `/sites/{id}/wordpress/{staging,clone,updates,security,cache}`;
  CLI `openpanel site wp {stage,clone,update,scan,cache}`; web WP tab
  (CSRF).
- Security: scans run read-only except for the permission-fix action
  (opt-in); rollback restores the prior WP version and DB snapshot;
  CVE feed is local and offline.
- Coupling: depends on `web-application-installer` to detect WP sites;
  reuses `site-staging` for staging and `add-site-clone-and-template-export`
  for clone.
