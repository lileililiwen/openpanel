# Refine panel navigation and UX — Design

## Explore & Reuse

Reused as-is (do not reintroduce):

- `crates/openpanel-web/src/layout.rs` — `Shell`, `CapabilitySet`
  (`shipped()` / `with()` / `contains()`), `RequiredRole` gating, the
  `csrf_field` helper, and the existing `nav-disclosure`
  `details`/`summary` mobile pattern (the same mechanism powers the new
  desktop section disclosures).
- `crates/openpanel-web/src/router.rs` — `WebState::render_shell`,
  `WebUser`; no handler changes.
- `crates/openpanel-web/src/assets.rs` — the stylesheet pipeline;
  `app.css` gains new component rules but keeps the token vocabulary.
- `tests/integration/web_ui_styling.rs` — the responsive/form/label
  contract is the regression net for the new chrome.

New code:

- `crates/openpanel-web/src/nav_model.rs` — the grouped navigation
  model: `NavSection { label, items }`, `NavItem { href, label,
  capability, role, icon }`, plus the icon set and the group map
  (moves `NAV_SECTIONS` out of `layout.rs`).
- A small inline vanilla-JS block in the Shell for the filter box and
  collapse/rail persistence (localStorage). No framework; HTMX stays.

## Group map

Current flat sections → new six-section model:

| New section        | Items |
| ------------------ | ----- |
| Overview           | Dashboard |
| Websites           | Sites, Files, SSL, Databases |
| Mail & Network     | Mail, Webmail, DNS, Security |
| Operations         | Monitoring, Logs, Backups, Cron, Services |
| Apps               | Software Center, Plugin Marketplace, Plugins |
| System             | Containers, Container quota, Container Registry, Registry credentials, Users, Branding, Settings |

Rules carried over from today:

- Item visibility = `capabilities.contains(item.capability) &&
  item.role.allows(user.role)`; a section with zero visible items is
  omitted (existing `visible_items` logic).
- Active state + breadcrumb resolution keyed on the item `href`
  (existing `is_active` / `current_item`).

## Client-side behaviour

- **Collapse**: each section renders as
  `<details class="nav-section"><summary>…</summary>…</details>` with a
  `localStorage` key (`openpanel.nav.<section>.open`) read on render
  and written on toggle. Default open on first visit.
- **Filter**: an `<input>` above the sections narrows items by label
  substring; sections with no match are hidden while a query is active
  (filter state is not persisted).
- **Compact rail**: a toggle writes `openpanel.nav.rail=1`;
  in rail mode the sidebar renders icons only and reveals the label in
  an overlay on `:hover`/`:focus-visible`. Disabled below the mobile
  breakpoint (the existing `nav-disclosure` takes over).

## Security & privacy

- The filter and collapse code never touches page content or audit
  data; no new endpoints, no new cookies beyond localStorage keys, and
  no server-side state. No secrets rendered.

## Validation

- `make check` must stay green, including `web_ui_styling`
  (responsive/form/label contract) and the navigation unit tests in
  `layout.rs`.
