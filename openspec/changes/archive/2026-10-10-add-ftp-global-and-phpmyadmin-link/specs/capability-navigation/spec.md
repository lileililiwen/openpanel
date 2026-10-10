# capability-navigation Specification

## ADDED Requirements

### Requirement: Global FTP Is Discoverable From The Shell

The `/ftp` aggregate MUST have one registry entry (`key:
"ftp-global"`, route `/ftp`, capability `ftp`, label `FTP`,
scope Global, `min_role: User`) in the Websites group,
mounted in the web router, with role filtering as defense in
depth: the link hides when the `ftp` capability is unmounted
and direct access still returns the authorization response.
The existing site-scoped `site-ftp` entry is unchanged.

#### Scenario: User sees the FTP link

- **WHEN** a User loads the shell with the `ftp` capability
  mounted
- **THEN** the Websites group renders the `FTP` link
  resolving to the mounted `/ftp` route.

#### Scenario: Unmounted capability hides the link

- **WHEN** the `ftp` capability is not mounted
- **THEN** no active FTP link renders and the route contract
  names the missing entry.
