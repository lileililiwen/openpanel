# Refine web-ui with audit, accessibility, theming — Design

## Audit route stub

```rust
// openpanel-app/src/web/audit_route.rs
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/audit", get(audit_index))
        .route("/audit/events", get(audit_list))
}

async fn audit_index() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED.with_header("x-openpanel-stub", "audit-ui-pending")
}
async fn audit_list() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}
```

Follow-on change `add-log-viewer` replaces the handlers.

## Theme tokens

```css
/* crates/openpanel-web/assets/tokens.css */
:root {
  --op-color-bg: var(--op-color-bg-default, #0f1115);
  --op-color-fg: var(--op-color-fg-default, #e7eaee);
  --op-color-accent: var(--op-color-accent-default, #4f8cff);
  --op-radius-md: 6px;
  --op-space-2: 8px;
  --op-space-4: 16px;
  /* etc. */
}
```

Templates use `class="…bg-[color:var(--op-color-bg)]…"` or via CSS
classes sourced from `tokens.css`. Lint enforced by the
quality follow-on change.

## String table

```rust
// crates/openpanel-web/src/t.rs
pub fn t(key: &str) -> String { t_lookup(key) }   // stub
fn t_lookup(key: &str) -> String { key.to_string() } // stub
```

The stub returns the key literally; `add-i18n-and-localization`
adds the table loader.

## Accessibility floor

- Every `<form>` MUST have an associated `<label>`.
- Every `<a>` / `<button>` MUST have accessible text.
- Colour contrast on tokens MUST meet WCAG 2.1 AA.
- The user MUST be able to navigate the entire shell by keyboard
  alone (tab order, focus rings, skip-to-content link).

## Tests

```
1.1  Unit: tokens.css is the only place colors are declared;
      absence of literal user-visible strings in templates is
      asserted by a content scan over compiled HTML.
1.2  Property: every route in the audit group returns 501 from
      the stub.
1.3  Service tests: the audit stub returns 501 with no data
      leakage.
```
