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

