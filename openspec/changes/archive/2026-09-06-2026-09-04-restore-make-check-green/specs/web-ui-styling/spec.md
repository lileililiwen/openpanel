# Delta for Web UI Styling

## ADDED Requirements

### Requirement: Filter Forms Are In Contract Scope

Filter forms — a `<form method="get">` that narrows a listing and carries
no CSRF token — SHALL be covered by the Global Form Contract and Paired
Labels requirements, and SHALL be walked by
`tests/integration/web_ui_styling.rs` like any other form.

#### Scenario: A filter form declares an ad-hoc class

- **WHEN** a filter form declares a one-off class token such as
  `audit-filters` alongside the global `form` class
- **THEN** `tests/integration/web_ui_styling.rs` fails and names the
  route and the offending token.

#### Scenario: A filter control has no paired label

- **WHEN** a visible input, select, or textarea in a filter form has
  neither a wrapping `<label>` nor a matching `<label for="...">`
- **THEN** `tests/integration/web_ui_styling.rs` fails and names the
  route and the control.

#### Scenario: A placeholder merely repeats its label

- **WHEN** a control's `placeholder` text is identical to the text of
  its paired label
- **THEN** the placeholder is dropped so the text is not rendered twice.
