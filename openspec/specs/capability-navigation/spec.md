# capability-navigation Specification

## Purpose

The shell has capability filtering and the site workspace has shared tabs, but the router, capability inventory, navigation, and tab definitions are separate lists. This causes mounted workflows to be undiscoverable and causes site tabs for Domains, Runtime, Logs, and Backups to be intentionally absent.
## Requirements
### Requirement: One Discoverability Inventory

Every mounted first-class web workflow MUST have one metadata entry declaring
its route, capability, label, icon, scope, and minimum role.

#### Scenario: Mounted workflow missing metadata

- **WHEN** a first-class route is mounted without registry metadata
- **THEN** the route contract fails before release.

### Requirement: Navigation and Router Agree

Navigation links MUST resolve to mounted routes, and unmounted capabilities
MUST never render as active links.

#### Scenario: Dead navigation link

- **WHEN** a navigation item points to an unmounted route
- **THEN** the discoverability test fails and names the item.

### Requirement: Site Workspace Is Complete and Scoped

Site-scoped workflows MUST appear in the site workspace when mounted and
authorized, preserving site identity across child pages and errors.

#### Scenario: Authorized runtime tab

- **WHEN** the runtime capability is mounted and a user can manage the site
- **THEN** the Runtime tab links to the site-scoped runtime route.

### Requirement: Role Filtering Is Defense in Depth

Owner-only navigation MUST be absent for lower roles, while direct access to
the route MUST still return the authorization response.

#### Scenario: User requests owner route directly

- **WHEN** a User calls an owner-only route URL
- **THEN** the handler denies the request without leaking the workflow.

### Requirement: Migration Wizard Is Discoverable From The Shell

The `/migrate` wizard MUST have one registry entry (`key:
"migrate"`, route `/migrate`, capability `migration`, label
`Migrate`, scope Global, `min_role: Owner`) in the Operations
group, mounted in the web router, with role filtering as
defense in depth: the link hides below `Owner` and direct
access still returns the authorization response.

#### Scenario: Owner sees the Migrate link

- **WHEN** an Owner loads the shell with the `migration`
  capability mounted
- **THEN** the Operations group renders the `Migrate` link
  resolving to the mounted `/migrate` route.

#### Scenario: User is denied direct access

- **WHEN** a User-role caller requests `/migrate` directly
- **THEN** the handler denies the request without leaking plan
  or bundle content.

### Requirement: Terminal Page Is Discoverable From The Shell

The `/terminal` page MUST have one registry entry (`key:
"terminal"`, route `/terminal`, capability `terminal`, label
`Terminal`, scope Global, `min_role: Owner`) in the Operations
group, mounted in the web router, with role filtering as
defense in depth: the link hides below `Owner` and direct
access still returns the authorization response.

#### Scenario: Owner sees the Terminal link

- **WHEN** an Owner loads the shell with the `terminal`
  capability mounted
- **THEN** the Operations group renders the `Terminal` link
  resolving to the mounted `/terminal` route.

#### Scenario: User is denied direct access

- **WHEN** a User-role caller requests `/terminal` directly
- **THEN** the handler denies the request without leaking
  server credentials or command bodies.

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

### Requirement: Dashboard Deep-Links Resolve To Registry

Every dashboard deep-link added by this change (Site/FTP/DB counts,
security fix link, software shortcuts, traffic drill-down, panel
shortcuts, quota links) MUST resolve via the discoverability registry
to a mounted route with matching capability, label, icon, and minimum
role. No new route is mounted by this change.

#### Scenario: Dashboard link agreement

- **WHEN** the dashboard renders a deep-link
- **THEN** `find_by_route` returns a registry entry for it and the
  discoverability test names any link that does not resolve.

#### Scenario: Role-gated link hidden but denied

- **WHEN** a lower-role caller views an owner-only dashboard card
- **THEN** the link is absent and direct access still returns the
  authorization response.

