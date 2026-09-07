# Spec delta: quality

## MODIFIED Requirements

### Requirement: Template-Literal Scan

The repository SHALL ship a content scanner that fails `make check` whenever a literal hex / rgb / hsl colour value or a literal user-visible English string appears in any path that has not been approved by the web-ui spec. The scanner's allowlist SHALL be `tokens.css` for colours and `t.rs` for strings; every other path that contains a literal — including `crates/openpanel-web/assets/app.css` — is a violation. The scanner MUST be included in `make check`.

#### Scenario: Literal hex in a template

- **WHEN** a developer writes `style="color: #4f8cff"` inside a `maud` template
- **THEN** `make check` fails with `scan: literal hex color outside tokens.css` and the offending path and line number.

#### Scenario: Same literal in tokens.css

- **WHEN** the same colour is declared inside `tokens.css`
- **THEN** the scanner accepts it.

#### Scenario: Literal user-visible string in a template

- **WHEN** a developer writes `<button>Save</button>`
- **THEN** the scanner fails with `scan: literal user-visible string in template body`.

#### Scenario: Literal hex in app.css fails the scan

- **WHEN** a developer writes `color: #ff00ff;` inside `crates/openpanel-web/assets/app.css`
- **THEN** `make check` fails with `scan: literal hex color outside tokens.css` and the offending file and line number.

#### Scenario: Literal rgba in app.css fails the scan

- **WHEN** a developer writes `background: rgba(79, 140, 255, 0.1);` inside `crates/openpanel-web/assets/app.css`
- **THEN** the scanner fails with `scan: literal rgb color outside tokens.css` and the offending file and line number. The fix is to declare `--op-color-accent-rgb: 79, 140, 255;` in `tokens.css` and reference it as `rgba(var(--op-color-accent-rgb), 0.1)`.

#### Scenario: Class-coverage gate is part of make check

- **WHEN** a developer adds `class="made-up-widget"` inside `crates/openpanel-web/src/`
- **THEN** `make check` fails with `step: class-coverage status: failed` and the file:token pair. The scanner does not modify user files; it only reports.
