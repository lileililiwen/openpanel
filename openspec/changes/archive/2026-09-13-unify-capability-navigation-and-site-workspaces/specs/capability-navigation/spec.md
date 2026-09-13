# capability-navigation Specification

## Requirements

## ADDED Requirements

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
