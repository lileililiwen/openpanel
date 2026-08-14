## ADDED Requirements

### Requirement: Audit Log Route Group

The web UI SHALL register a route group at `/audit` as a known shell route. In this change the route handlers return `501 Not Implemented` with a non-secret `x-openpanel-stub: audit-ui-pending` header; the follow-on `add-log-viewer` change replaces the handlers. The route MUST be reachable from the shell navigation only for Owners and Admins.

#### Scenario: Stub returns 501

- **WHEN** an authenticated Owner visits `/audit`
- **THEN** the response is `501` with `x-openpanel-stub: audit-ui-pending` and no payload leaks audit data.

#### Scenario: User role cannot reach the stub

- **WHEN** a User-role principal navigates to `/audit`
- **THEN** the response is `403`.

### Requirement: Accessibility Floor (WCAG 2.1 AA)

Every web UI route group registered in this change or any prior change SHALL conform to a baseline accessibility floor: every `<form>` has an associated `<label>`, every interactive element has accessible text, keyboard navigation covers the entire shell, focus indicators are visible, and colours meet the WCAG 2.1 AA contrast ratio. The lint rules for these floors are recorded in the `quality` capability; this change states the floors exist and is the canonical place to anchor follow-on accessibility work.

#### Scenario: Form has a label

- **WHEN** a developer adds a new form without a `<label>`
- **THEN** code review rejects the change under the accessibility requirement.

#### Scenario: Colour contrast meets 2.1 AA

- **WHEN** a new colour pair is added to `tokens.css`
- **THEN** the contrast ratio test in the follow-on accessibility change passes.

### Requirement: Theme Tokens Contract

The web UI SHALL declare colours, spacing, and typography in a single `tokens.css`. No template or component CSS file may declare a literal colour, spacing, or font-size value; every such value MUST source a CSS custom property declared in `tokens.css`. The `themeable-ui` follow-on change adds per-reseller overrides.

#### Scenario: Template uses token

- **WHEN** a template previously wrote `style="color: #4f8cff"`
- **THEN** it now uses `style="color: var(--op-color-accent)"`.

#### Scenario: Tokens are the single source of truth

- **WHEN** an additional non-token colour value is introduced
- **THEN** the corresponding lint (added by the quality follow-on change) flags it as `literal_color_value_outside_tokens`.

### Requirement: String Table Contract

The web UI SHALL source every user-visible string from a typed `T(key)` lookup. Templates MUST NOT contain literal user-visible strings as direct text or in attributes that are exposed to users (`title`, `aria-label`, `alt`, `placeholder`). The stub in this change returns the key literally; the `add-i18n-and-localization` follow-on change implements lookup.

#### Scenario: Literal string rejected

- **WHEN** a developer writes `<button>Save</button>` directly
- **THEN** review rejects it; the template rewrites to `<button>{{ t("save_button") }}</button>`.

#### Scenario: Stub returns the key

- **WHEN** this change is the only shipped change and the page renders
- **THEN** the page shows the symbol `save_button` instead of "Save".
