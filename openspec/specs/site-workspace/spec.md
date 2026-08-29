# site-workspace Specification

## Purpose
TBD - created by archiving change add-site-workspace-ux. Update Purpose after archive.
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

