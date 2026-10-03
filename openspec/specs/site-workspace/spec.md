# site-workspace Specification

## Purpose

OpenPanel currently distributes site operations across separate pages and links. aaPanel and BaoTa make the site the primary workspace, grouping domains, runtime, files, SSL, redirects, proxy, security, logs, backups, and deployment actions.
## Requirements
### Requirement: Site operations share a workspace

The site detail page MUST provide capability-filtered navigation for overview, domains, files, SSL, runtime/HTTP controls, security, logs, backups, and deployment surfaces.

#### Scenario: Site has supported modules

- **WHEN** an authorized owner opens a site
- **THEN** supported site operations appear as workspace tabs with the site context preserved

### Requirement: Unsupported tabs are omitted

The workspace MUST omit unavailable capabilities instead of rendering dead links.

#### Scenario: FTP is unavailable

- **WHEN** FTP is not registered
- **THEN** the FTP tab is absent while other tabs remain usable

### Requirement: Site context and authorization are preserved

Every workspace route MUST enforce site ownership/permission and preserve site breadcrumbs after success or error.

#### Scenario: User opens another owner's site

- **WHEN** a user requests a foreign site workspace route
- **THEN** access is denied without revealing the site's metadata

