# Add Web UI styling and responsive layout — Design

## Form contract

``` CSS
/* Global form rules. Every <form> MUST use one of these classes.
   No inline styles, no per-form class names. */

form {
    max-width: 480px;
    display: flex;
    flex-direction: column;
    gap: var(--op-space-3);
    background: var(--op-color-surface);
    border: 1px solid var(--op-color-border);
    border-radius: var(--op-radius-md);
    padding: var(--op-space-4);
    margin-block: var(--op-space-4);
}

form label {
    color: var(--op-color-fg-muted);
    font-size: var(--op-font-size-sm);
    display: flex;
    flex-direction: column;
    gap: var(--op-space-1);
}

form input[type="text"],
form input[type="email"],
form input[type="password"],
form input[type="number"],
form input[type="search"],
form input[type="tel"],
form input[type="url"],
form textarea,
form select {
    padding: var(--op-space-2) var(--op-space-3);
    border: 1px solid var(--op-color-border);
    border-radius: var(--op-radius-md);
    background: var(--op-color-bg);
    color: var(--op-color-fg);
    font: inherit;
}

form input:focus-visible,
form textarea:focus-visible,
form select:focus-visible {
    outline: 2px solid var(--op-color-focus-ring);
    outline-offset: 1px;
}

form button[type="submit"] {
    margin-top: var(--op-space-2);
    padding: var(--op-space-3);
    border: 0;
    border-radius: var(--op-radius-md);
    background: var(--op-color-accent);
    color: var(--op-color-accent-fg);
    font: inherit;
    cursor: pointer;
}

form button[type="submit"]:hover { background: var(--op-color-accent-strong); }

/* Two-column layout for wider forms. */
form.form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--op-space-3);
}

form.form-grid button[type="submit"] {
    grid-column: 1 / -1;
}

/* Single-row input + button (used for "Add record", "Run now", ...). */
form.form-row {
    flex-direction: row;
    align-items: end;
    flex-wrap: wrap;
}
```

Every `<form>` in the templates declares one of three classes
(`form`, `form form-grid`, `form form-row`). The class is the
contract; reviewers MUST reject any new template that uses an
ad-hoc name.

## Three-breakpoint responsive rules

``` CSS
/* Default = mobile (< 640 px). The shell stacks vertically. */
.layout {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
}

.main {
    display: flex;
    flex-direction: column;
    min-width: 0;
}

/* Tablet ≥ 640 px: collapsible sidebar, content fits without
   horizontal scroll. */
@media (min-width: 640px) {
    .layout {
        display: grid;
        grid-template-columns: 220px minmax(0, 1fr);
    }
    .nav-disclosure {
        border-right: 1px solid var(--op-color-border);
        border-bottom: 0;
    }
    .nav-disclosure > summary { display: none; }
}

/* Desktop ≥ 1024 px: wider sidebar, larger spacing. */
@media (min-width: 1024px) {
    .layout { grid-template-columns: 260px minmax(0, 1fr); }
    form { max-width: 720px; }
}
```

`max-width` media queries are forbidden (they cascade incorrectly
on in-between widths). CI lints `app.css` and fails on
`@media (max-width: ...)`.

## Tables on narrow screens

Wide tables switch to a horizontal scroll inside a card so the
page itself does not scroll:

``` CSS
.table-card {
    overflow-x: auto;
    border: 1px solid var(--op-color-border);
    border-radius: var(--op-radius-md);
    background: var(--op-color-surface);
}

.table-card table { width: 100%; min-width: 640px; }
```

## CI enforcement

The test suite `tests/integration/web_ui_styling.rs` checks:

1. Every page contains `<form class="…">` (no bare `<form>`).
2. Every `<input>` is paired with a `<label>`.
3. Loading any page at 360 / 768 / 1280 px width produces no
   horizontal scroll on `<body>` (assert via DOM measurement).
4. `app.css` contains no `@media (max-width: …)` and no literal
   hex colours outside `tokens.css` (regex scan).

The CI gate fails on any regression. New PRs cannot merge until
the gate is green.

## Migration plan

Each `<form>` template is migrated in this order:

1. `crates/openpanel-web/src/login.rs` — canonical example.
2. `crates/openpanel-web/src/sites.rs` (site create).
3. `crates/openpanel-web/src/databases.rs` (database create).
4. `crates/openpanel-web/src/api_tokens.rs`,
   `crates/openpanel-web/src/mail.rs`,
   `crates/openpanel-web/src/dns.rs`,
   `crates/openpanel-web/src/ftp.rs`.
5. `crates/openpanel-web/src/cron.rs`,
   `crates/openpanel-web/src/docker.rs`,
   `crates/openpanel-web/src/db_pitr.rs`,
   `crates/openpanel-web/src/two_factor.rs`,
   `crates/openpanel-web/src/collaborators.rs`,
   `crates/openpanel-web/src/container_registry.rs`,
   `crates/openpanel-web/src/plugin_marketplace.rs`,
   `crates/openpanel-web/src/notifications.rs`,
   `crates/openpanel-web/src/security.rs`,
   `crates/openpanel-web/src/settings.rs`.

`sites::row_action_forms` (the `class="inline"` row forms) are
rendered with the same global `form` class but constrained to a
single button via `form.form-row`.

## Endpoints

(no new panel endpoints; this is a UI-only change)

## Tests

```
1.1 Unit: every existing <form> in the templates carries a class
    from the global set (form / form form-grid / form form-row).
1.2 Property: scanning app.css finds zero max-width queries.
1.3 Service: app.css + tokens.css render correctly at every page
    route.
1.4 Integration: tests/integration/web_ui_styling.rs loads each
    page at three widths; no horizontal overflow on body.
1.5 Web: every form has a paired label and an input focus ring.
```