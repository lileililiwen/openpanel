## ADDED Requirements

### Requirement: Global Form Contract

Every `<form>` element in the web UI MUST declare a class from the
global set: `form`, `form form-grid`, or `form form-row`. The class
sources every colour, spacing, and typography value from
`tokens.css`; the form MUST NOT declare a literal value, a one-off
class name, or rely on browser defaults for any descendant element.

#### Scenario: Form has the global class

- **WHEN** any web route renders a `<form>`
- **THEN** the form declares `class="form"` (or `form form-grid`,
        or `form form-row`) and no descendant declares a literal
        colour or a non-token spacing value.

#### Scenario: Form has no unstyled inputs

- **WHEN** a developer adds a `<form>` without the global class
- **THEN** `tests/integration/web_ui_styling.rs` fails the build
        with the offending file and the missing class.

### Requirement: Paired Labels

Every `<input>`, `<textarea>`, and `<select>` MUST be paired with a
`<label>` either via a wrapping `<label>` or an explicit
`for`/`id` association. The visual focus ring MUST be visible via
`:focus-visible`.

#### Scenario: Input has a label

- **WHEN** the rendering pipeline encounters an input
- **THEN** a sibling or parent `<label>` exists with text content;
        the `:focus-visible` rule paints a 2 px ring in
        `var(--op-color-focus-ring)`.

### Requirement: Three-Breakpoint Responsive Layout

Every public web route MUST render without horizontal overflow at
the three breakpoints: 360 px (mobile), 768 px (tablet), and 1280 px
(desktop). The CSS uses the mobile-first `min-width` pattern;
`max-width` media queries are forbidden.

#### Scenario: Mobile viewport fits

- **WHEN** a page is rendered at 360 px
- **THEN** `<body>` has zero horizontal overflow and every form
        control is at least 44 px tall (touch target floor).

#### Scenario: Desktop viewport uses full width

- **WHEN** a page is rendered at 1280 px
- **THEN** the layout uses the wider sidebar and the form max-width
        matches the design tokens (480 / 720 px).

### Requirement: Token-Only Styling

Every CSS rule outside `tokens.css` MUST source colours, spacing,
radii, and typography from the token custom properties declared in
`tokens.css`. Literal hex codes, pixel-spacing values, or non-token
font families are forbidden outside `tokens.css`.

#### Scenario: CSS uses tokens

- **WHEN** the quality scan runs over `app.css`
- **THEN** the scan reports zero literal hex colours, zero
        `@media (max-width: …)` queries, and zero non-token pixel
        spacing values; CI fails otherwise.

### Requirement: Tables Scroll Inside a Card

Wide tables MUST be wrapped in a `.table-card` container that adds
horizontal overflow scrolling. The page itself MUST NOT scroll
horizontally on a narrow viewport.

#### Scenario: Narrow viewport, wide table

- **WHEN** a table wider than the viewport renders
- **THEN** the table fits inside a `.table-card` with
        `overflow-x: auto`; the page body remains within the
        viewport width.