# Refine panel navigation and UX — Tasks

## 1. Testing

- [ ] 1.1 Unit tests for the group model: every item appears in
      exactly one section; section order is fixed; a section with
      no visible items is omitted.
- [ ] 1.2 Unit tests: every `NavItem` has an icon in the built-in
      icon set; unknown icon names fail to compile or render.
- [ ] 1.3 Unit tests: collapsible sections render
      `details`/`summary`, restore persisted open state, and write
      the localStorage key on toggle.
- [ ] 1.4 Unit tests: the sidebar filter narrows items by label
      substring and hides empty sections while a query is active.
- [ ] 1.5 Unit tests: compact rail mode renders icons only and
      reveals labels on hover/focus; disabled on narrow viewports.
- [ ] 1.6 Unit tests: capability/role gating still applies to the
      reorganized groups (existing `visible_items` behaviour).
- [ ] 1.7 Integration: Owner sees the six groups with icons and
      active-state highlighting (web_ui navigation test).
- [ ] 1.8 Integration: non-owner role sees filtered groups;
      `web_ui_styling` responsive/form/label contract passes for
      every public route.

## 2. Layout Model

- [ ] 2.1 Add `nav_model.rs` with `NavSection` / `NavItem` model,
      the group map, and `CapabilitySet` re-exports; keep the
      public `CapabilitySet` API stable for `handlers.rs`.
- [ ] 2.2 Add the inline-SVG icon set keyed by nav item.
- [ ] 2.3 Rework `Shell` rendering: grouped sections with
      disclosure, filter box, and compact rail.
- [ ] 2.4 Wire localStorage persistence and the filter box with a
      small vanilla-JS block in the shell.

## 3. Styling

- [ ] 3.1 Add `app.css` rules for the rail, filter, disclosure,
      and icons using existing design tokens.
- [ ] 3.2 Confirm the responsive contract (no fixed pixel widths,
      mobile-first breakpoints) still holds.

## 4. Validation

- [ ] 4.1 `make check` clean.
- [ ] 4.2 Smoke-test: log in as Owner, collapse a section, use the
      filter, toggle the rail, reload and confirm persistence.
- [ ] 4.3 Archive with `openspec archive
      2026-08-18-refine-panel-navigation`.
