## ADDED Requirements

### Requirement: WordPress Staging

For a site detected as WordPress, `POST /sites/{id}/wordpress/staging`
SHALL create a staging copy by reusing the `site-staging` mechanism,
including the docroot and database, and rewrite `wp-config` for the new
URL/prefix. The staging target SHALL remain inside panel-managed space.

#### Scenario: Staging created

- **WHEN** an Owner stages WP site `s1`
- **THEN** a new staging site exists with a copied docroot + DB, a
        rewritten `wp-config`, and audit `WpStaged{target_site_id}`
        records the id only.

### Requirement: WordPress Clone

`POST /sites/{id}/wordpress/clone` SHALL clone a WP site into a new site
by reusing `add-site-clone-and-template-export`, copying both the
docroot and database.

#### Scenario: Clone created

- **WHEN** an Owner clones WP site `s1` with `{ target_name: "shop" }`
- **THEN** a new site `shop` exists with WP files + DB copied, and audit
        `WpCloned{target_site_id}` records the id only.

### Requirement: Update Management with Rollback

`POST /sites/{id}/wordpress/updates` SHALL apply core/plugin/theme
updates after snapshotting the database and docroot, and SHALL roll back
to the snapshot if a post-update health check fails.

#### Scenario: Successful update

- **WHEN** an Owner updates core + a plugin on `s1`
- **THEN** the new versions are active and audit `WpUpdated{set}`
        records names only (no file contents).

#### Scenario: Failed update rolls back

- **WHEN** a post-update health check fails
- **THEN** the DB snapshot and docroot are restored and audit
        `WpUpdateRolledBack{run_id}` is recorded.

### Requirement: Security Scan

`GET /sites/{id}/wordpress/security` SHALL return a report covering file
permissions, version currency, and known vulnerabilities matched against
a local CVE feed. The scan SHALL be read-only except for an explicit,
opt-in permission-fix action.

#### Scenario: Scan reports issues

- **WHEN** an Owner scans WP site `s1` with an outdated plugin and a
        CVE-listed version
- **THEN** the `WpSecurityReport` lists `perms_ok`, `outdated`, and
        `known_vulns` without modifying files.

### Requirement: Cache Toggle

`PUT /sites/{id}/wordpress/cache` SHALL switch the WP cache mode
(`off` / `object` / `page`) by writing the relevant `wp-config`
constants and flushing caches. Only the three known modes SHALL be
accepted.

#### Scenario: Cache mode changed

- **WHEN** an Owner sets `{ mode: "object" }`
- **THEN** `wp-config` cache constants reflect object caching, caches
        are flushed, and audit `WpCacheChanged{mode}` records the mode
        only.

#### Scenario: Unknown mode rejected

- **WHEN** an unsupported `mode` is supplied
- **THEN** the request is rejected with `WpError::UnknownCacheMode` and
        no config is written.
