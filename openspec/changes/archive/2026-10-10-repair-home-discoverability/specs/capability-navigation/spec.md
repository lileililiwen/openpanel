# capability-navigation Specification

## ADDED Requirements

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
