# browser-ui-quality Specification

## Requirements

## ADDED Requirements

### Requirement: Rendered Accessibility Gate

Every registered route in the browser matrix MUST pass the WCAG 2.2 AA
keyboard, focus, accessible-name, state, contrast, target-size, and
focus-obscured checks for its supported roles.

#### Scenario: Missing focus indicator

- **WHEN** a keyboard user focuses an interactive control
- **THEN** the browser gate fails if focus is not visible or is obscured.

### Requirement: Responsive Route Gate

Every registered route MUST render without horizontal overflow or fixed-width
layout failure at 360px, 768px, and 1280px viewports.

#### Scenario: Mobile overflow

- **WHEN** a route is rendered at 360px
- **THEN** no primary content requires horizontal scrolling outside an approved
  data-table container.

### Requirement: Typed Localization

User-visible template text and exposed attributes MUST resolve through typed
translation keys with deterministic locale fallback and formatting.

#### Scenario: Missing locale key

- **WHEN** a selected locale lacks a key
- **THEN** the fallback locale renders a readable value and the test reports
  the missing key for catalog completion.

### Requirement: Reduced Motion

Interactive and loading motion MUST respect `prefers-reduced-motion: reduce`
without removing status information or keyboard operation.

#### Scenario: Reduced-motion user

- **WHEN** the browser requests reduced motion
- **THEN** transitions/animations are suppressed while state changes remain
  perceivable.
