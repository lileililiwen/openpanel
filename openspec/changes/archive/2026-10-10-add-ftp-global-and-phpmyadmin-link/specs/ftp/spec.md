# ftp Specification

## ADDED Requirements

### Requirement: Global FTP Aggregate List

The web UI MUST render `GET /ftp` inside the shell: one row
per FTP account across all sites visible to the caller
(site, username, home, mode, status), each row linking to its
site-scoped `/sites/{id}/ftp` page. The page MUST reuse
`FtpService::list` per visible site, MUST enforce the same
site ownership rules as the per-site page, and MUST never
render credential material.

#### Scenario: Owner views the aggregate

- **WHEN** an Owner with FTP accounts on two sites requests
  `GET /ftp`
- **THEN** the server renders both accounts with their site
  links and no plaintext passwords.

#### Scenario: User sees only accessible sites

- **WHEN** a User with access to one of two sites requests
  `GET /ftp`
- **THEN** the response contains only that site's accounts.

#### Scenario: Empty aggregate

- **WHEN** the caller has no visible FTP accounts
- **THEN** the page renders an empty state linking to the
  visible sites' FTP pages.
