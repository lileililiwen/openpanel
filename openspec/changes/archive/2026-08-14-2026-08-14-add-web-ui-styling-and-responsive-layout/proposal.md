# Add Web UI styling and responsive layout

## Why

The web UI today has only **ad-hoc, opt-in form classes** (`.form`,
`.login`) and exactly **one** `@media (max-width: 768px)` block that
only collapses the sidebar. The result is two compounding
regressions:

1. **Inputs are unstyled.** Most `<form>` elements ship without a
   class at all (`crates/openpanel-web/src/cron.rs`,
   `docker.rs`, `mail.rs`, `api_tokens.rs`, `two_factor.rs`,
   `db_pitr.rs`, …), so their `<input>` / `<textarea>` / `<button>`
   fall back to the browser's default look. New pages silently
   regress to the ugly default because there is no global contract.
2. **The layout is desktop-only.** A 360 px mobile viewport gets a
   220 px sidebar plus an oversized content area that forces
   horizontal scroll. There is no tablet breakpoint either.

The follow-on `add-themeable-ui-and-white-label` change adds token
*vocabulary* (`tokens.css`) but no global **form**, **grid**, or
**responsive layout** rules. This change ships that vocabulary as
a real contract: every `<form>` MUST use the shared classes, every
page MUST work at three breakpoints, and CI MUST fail when a
regression slips through.

## What Changes

- New bounded context `web-ui-styling` carrying the design contract:
  the `form`, `form-row`, `form-grid`, `card`, and `table-card` CSS
  classes, the three-breakpoint responsive rules, the focus-visible
  ring, and the CI contract (no unstyled `<form>`, no
  `max-width` queries, no colours outside `tokens.css`).
- Refactor every existing `<form>` in `crates/openpanel-web/src/`
  to use the new global class. The legacy `.form` / `.login` opt-in
  is removed; the global rule owns the styling.
- New `tests/web_ui_styling.rs` integration suite that loads each
  public web route at 360 / 768 / 1280 px and asserts no horizontal
  overflow, every `<form>` carries a class, and every `<input>`
  has a paired `<label>`.

## Capabilities

### New Capabilities

- `web-ui-styling`: a global form-styling and responsive-layout
  contract with CI enforcement. Every web route ships styled inputs
  and a layout that adapts at 360 / 768 / 1280 px without
  horizontal scroll.

## Impact

- Domain: none (this is purely a CSS / template contract).
- App: none (no backend changes).
- Web: `crates/openpanel-web/assets/app.css` is rewritten against
  `tokens.css`; every `<form>` template is migrated to the global
  class; the responsive media queries are added.
- Test: `tests/integration/web_ui_styling.rs` checks form classes,
  paired labels, and horizontal overflow at the three breakpoints.
- Coupling: depends on the tokens vocabulary from
  `refine-web-ui-with-audit-accessibility-theming` (already
  shipped) for colour / spacing / typography.