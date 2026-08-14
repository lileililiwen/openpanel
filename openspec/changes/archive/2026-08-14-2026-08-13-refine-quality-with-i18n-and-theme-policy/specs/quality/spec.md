## ADDED Requirements

### Requirement: Template-Literal Scan

The repository SHALL ship a content scanner that fails `make check` whenever a literal hex / rgb / hsl colour value or a literal user-visible English string appears in any path that has not been approved by the web-ui spec. The scanner's allowlist SHALL be `tokens.css` for colours and `t.rs` for strings; every other path that contains a literal is a violation. The scanner MUST be included in `make check`.

#### Scenario: Literal hex in a template

- **WHEN** a developer writes `style="color: #4f8cff"` inside a `maud` template
- **THEN** `make check` fails with `scan: literal hex color outside tokens.css` and the offending path and line number.

#### Scenario: Same literal in tokens.css

- **WHEN** the same colour is declared inside `tokens.css`
- **THEN** the scanner accepts it.

#### Scenario: Literal user-visible string in a template

- **WHEN** a developer writes `<button>Save</button>`
- **THEN** the scanner fails with `scan: literal user-visible string in template body`.

### Requirement: i18n Disallowed Formatters

`clippy.toml` SHALL declare a `disallowed-methods` entry for any locale-ignorant formatter introduced by the follow-on i18n change; in this change, only English-only `format!` in code paths that take a user-facing context is restricted. No production call site may construct the user-visible sentence by raw `format!()` once the `add-i18n-and-localization` change ships; users of `format!` for logs, errors, or internal formatting continue to be allowed.

#### Scenario: Log message uses format

- **WHEN** an internal log line uses `format!("disk used {} bytes", used)`
- **THEN** linting is silent (allowed).

#### Scenario: User-visible string uses format

- **WHEN** a developer introduces `format!("Welcome, {}!", user)` for a UI sentence
- **THEN** the `add-i18n-and-localization` change's lint rejects it and the call site rewrites to `f!("welcome_user", user)`.

### Requirement: Accessibility Gate

The repository SHALL ship a `make a11y` target that runs axe-core against the running dev server when available. The gate MUST be included in CI but MUST skip when no dev server is reachable, preserving local-friendliness.

#### Scenario: Dev server reachable

- **WHEN** `make a11y` runs and `http://127.0.0.1:8080/healthz` returns 200
- **THEN** axe-core runs against the reachable pages and exits 0 only if no serious or critical violations are present.

#### Scenario: Dev server unreachable

- **WHEN** `make a11y` runs and `127.0.0.1:8080` is closed
- **THEN** the target prints `a11y: skipped (no dev server)` and exits 0.
