# Design: Repair UI discoverability and capability registration

## Explore & Reuse

- Reuse `openpanel_web::layout::CapabilitySet`, `Shell::visible_items`, and `Shell::is_active`.
- Reuse `NAV_SECTIONS` and `icon_path`; do not add a second menu definition.
- Reuse router composition in `openpanel-web/src/router.rs` and existing role guards.
- Use `scripts/repo-map.sh` and the existing `web_ui.rs`/`web_ui_styling.rs` integration harness.

## Model

Keep `NavItem.capability` as the single declarative key. Add a canonical shipped capability list or adapter owned by the web composition root. Every mounted full-page web route must either map to one `NavItem` or be explicitly classified as a child/detail route. Unknown capability keys, duplicate paths, missing icons, and mounted-but-undiscoverable pages are errors.

The registry must be passed through both production and test composition roots. Role filtering remains presentation-only; authorization remains enforced by handlers.

## UX behavior

- Show all available first-class workflows for the caller.
- Keep the current item highlighted for nested paths.
- Keep mobile disclosure and compact rail behavior.
- Provide a visible fallback label if an icon lookup fails, but fail tests so the defect cannot ship.

## Verification

Unit tests cover registry completeness, icon completeness, duplicate routes, and nested active states. Integration tests fetch the owner and user shell, verify expected links, verify owner-only links are absent for users, and verify every visible link resolves to a registered route or documented child route.
