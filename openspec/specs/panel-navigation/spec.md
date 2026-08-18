# panel-navigation Specification

## Purpose
TBD - created by archiving change 2026-08-18-refine-panel-navigation. Update Purpose after archive.
## Requirements
### Requirement: Section Grouping

The sidebar SHALL organize navigation items into named sections that
group related capabilities. The section order SHALL be fixed:
Overview, Websites, Mail & Network, Operations, Apps, System. Each
item SHALL belong to exactly one section, and a section with no
visible items SHALL be omitted from the rendered sidebar.

#### Scenario: Owner sees grouped sections in fixed order

- **WHEN** an Owner views the sidebar
- **THEN** the six sections render in the fixed order with every item
  grouped under its section.

#### Scenario: Empty section omitted

- **WHEN** no visible item belongs to a section for the caller
- **THEN** that section is not rendered at all.

### Requirement: Inline Icons

Every visible navigation item SHALL render an inline SVG icon from the
shell's built-in set, embedded server-side with no external request.
Each icon SHALL carry an accessible title matching the item label.

#### Scenario: All items carry an icon

- **WHEN** the sidebar renders
- **THEN** every visible item contains an `svg` element with a `title`
  child equal to the item label.

### Requirement: Collapsible Sections

Each section SHALL be collapsible through a keyboard-accessible
disclosure. The open/closed state SHALL persist per user and be
restored on subsequent loads; sections default to open on first visit.

#### Scenario: Collapsed state persists

- **WHEN** an Owner collapses the Operations section and reloads the
  page
- **THEN** the Operations section renders collapsed.

### Requirement: Navigation Search

The sidebar SHALL provide a filter field that narrows visible items
by label substring as the user types. While a query is active, a
section with no matching items SHALL be hidden; clearing the query
restores the full grouped sidebar.

#### Scenario: Filter narrows the sidebar

- **WHEN** a user types `ssl` in the sidebar filter
- **THEN** only items whose label contains `ssl` remain visible and
  sections without a match are hidden.

### Requirement: Compact Rail Mode

On wide screens the sidebar SHALL support a compact icon-only mode
toggled by the user. In compact mode only icons render; the item
label SHALL appear in an overlay on hover or keyboard focus. The mode
SHALL persist per user and SHALL be unavailable below the mobile
breakpoint.

#### Scenario: Rail mode reveals labels on hover

- **WHEN** an Owner enables compact mode and hovers an item
- **THEN** the sidebar shows only icons and the hovered item reveals
  its label in an overlay.

### Requirement: Capability and Role Gating

The reorganized navigation SHALL continue to filter every item by the
registered capability set and the caller's role, with no change to
the existing gating semantics.

#### Scenario: Unregistered capability stays hidden

- **WHEN** the runtime capability set does not contain `marketplace`
- **THEN** the Plugin Marketplace item is hidden for every role.

#### Scenario: Owner-only items hidden from users

- **WHEN** a non-owner user views the sidebar
- **THEN** Owner-gated items are omitted and their section is hidden
  when it becomes empty.

### Requirement: Responsive Styling Contract

The new sidebar chrome (rail, filter, disclosure, icons) SHALL use the
existing CSS token vocabulary, be mobile-first, and MUST NOT introduce
fixed pixel widths, preserving the responsive contract enforced by the
styling integration tests.

#### Scenario: Token-based chrome passes the contract

- **WHEN** the stylesheet is fetched and every public route is walked
- **THEN** no inline fixed `width` style appears and the token-based
  breakpoint rules still hold.

