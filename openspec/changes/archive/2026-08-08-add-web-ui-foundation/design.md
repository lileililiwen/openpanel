# Design: Add Web UI — Foundation

## Context

OpenPanel is a pure-Rust, layered workspace (`domain` → `app` → `api`/`cli`).
The API (`crates/openpanel-api`) already serves JSON under `/api/v1` and issues
an `openpanel_session` HttpOnly cookie on login. The composition root lives in
`openpanel-cli/src/handlers.rs::serve`, which builds `build_router(...)` and
serves it with axum.

The frontend must not break the project's core promise: **one static binary,
no script engine at runtime**. Every prior panel in this space (cPanel = Perl,
baota = PHP, CloudPanel = Node/PHP) runs server-side scripting; OpenPanel's
differentiator is that nothing executes at runtime. The web UI must preserve
that.

## Goals / Non-Goals

**Goals:**

- Zero Node/npm/WASM toolchain; the frontend is part of the Rust workspace.
- Reuse the existing session + cookie auth (no second identity system).
- Interactive navigation and forms via HTMX, vendored and embedded in the binary.
- Clean, consistent shell layout that every follow-up page reuses.
- CSRF protection for cookie-authenticated form posts.

**Non-Goals** (deferred to follow-ups):

- Resource pages. Dark mode. i18n. PWA / offline. Browser E2E in CI.

## Decisions

### 1. Server-rendered HTML via `maud`, no JS framework

**Decision**: Render pages with the `maud` macro crate (`html! { ... }`),
which compiles templates to Rust code at build time. The web layer lives in a
new crate `crates/openpanel-web`, mounted at `/` next to the `/api/v1` nest.

**Rationale**: `maud` gives type-checked, allocation-light HTML with zero
runtime template engine — HTML is just Rust functions. There is no template
file to hot-load, no interpreter, nothing at runtime. This is the strongest
possible fit for the "no script engine" positioning.

**Alternatives considered**:
- *React/Vue SPA + Vite* — rejected: requires Node, a build step, and ships a
  JS bundle; the project explicitly targets "no script engine at runtime".
- *Leptos/Dioxus/WASM* — rejected: compiles to a WASM runtime, violating the
  static-binary story and adding a heavy toolchain.
- *askama/tera runtime templates* — workable, but a runtime template engine is
  exactly the "script engine" smell the project avoids; `maud` keeps HTML in
  Rust.

### 2. HTMX for interactivity, vendored

**Decision**: Vendor `htmx.min.js` under `crates/openpanel-web/assets/`,
embed it with `include_bytes!`, and serve it at `/assets/htmx.min.js`. Pages
set `hx-boost` so navigation is an HTMX swap; forms use `hx-post` /
`hx-delete` with `hx-target` swaps. No application JS is written.

**Rationale**: HTMX expresses the panel's interactions (list → detail,
filter, delete, inline create) declaratively in HTML attributes. It is a
single, well-known ~50 KB file — not a framework the project must maintain.

### 3. Session auth reuse

**Decision**: The web layer authenticates through the **same** session
middleware the API uses. `POST /login` calls `IdentityService::login(...)` and
sets the identical `openpanel_session` HttpOnly cookie (`SESSION_COOKIE`,
`Path=/; SameSite=Lax; Max-Age=86400`). A `WebUser` extractor reads the
resolved `AuthSession` extension; missing auth on a page request redirects
(`302`) to `/login` instead of returning 401 JSON.

**Rationale**: One session store, one cookie, one `resolve_session` code path.
The browser simply uses the same cookie the API already issues — there is
literally one auth system.

### 4. Web router mounted at `/`

**Decision**: `crates/openpanel-web/src/router.rs` builds a `Router` with the
web routes (`/login`, `/logout`, `/assets/*`, and the shell root `/` which
redirects to the dashboard once it exists). The composition root merges it:

```rust
let app = build_router(identity, sites, databases, files, ssl, monitoring)
    .merge(openpanel_web::router(identity));
```

The `/api/v1` nest and the web routes share the server and the session
middleware; JSON endpoints stay where they are.

### 5. CSRF protection

**Decision**: Each session gets a random 32-byte CSRF token (stored in the
session row, generated on login). Every `<form>` embeds it in a hidden field;
a `ValidateCsrf` extractor on state-changing web handlers compares it to the
session's token. The cookie is `SameSite=Lax` (no cross-site POST carries the
cookie), and the token makes same-site subdomains/XSS-surfaced CSRF infeasible.

> **Implementation note**: the foundation ships an in-memory per-session
> `CsrfStore` (keyed by session id) rather than a session-row column, avoiding
> a schema migration in this milestone. It is scoped per-process, mirroring the
> in-memory session resolution; moving the token into the session row is a
> follow-up if the panel ever runs more than one web process.

**Rationale**: Cookie auth needs CSRF defense; SameSite=Lax alone is
insufficient against same-site request forging. A per-session token is simple,
stateless server-side, and trivially testable.

### 6. Static assets embedded

**Decision**: `htmx.min.js` and `app.css` live in
`crates/openpanel-web/assets/` and are served from `include_bytes!` via a
`/assets/{name}` handler with correct `Content-Type` + immutable cache headers.

**Rationale**: Keeps the binary self-contained (no `/etc` paths, no external
CDN dependency, works offline).

## Security

- Passwords are submitted over the form; the login handler passes them to
  `IdentityService::login` exactly like the API — never logged.
- CSRF tokens are per-session, random, and never returned in JSON responses.
- The session cookie is HttpOnly + SameSite=Lax; the web layer never reads it
  in JS (there is no application JS).
- No user-supplied string is interpolated into HTML without escaping — `maud`
  escapes by default; where raw HTML is injected (none in foundation) a
  reviewed `PreEscaped` wrapper is used.

## Test strategy (pure Rust)

- Unit tests in `crates/openpanel-web` (`#[cfg(test)]`) for layout/login/CSRF
  rendering and the `WebUser`/csrf extractor behavior.
- Integration tests in `tests/integration/web_ui.rs` (via `TestServer`):
  login sets the cookie, unauthenticated `/` redirects, authenticated shell
  renders, logout clears the session, CSRF mismatch rejects the POST.
- E2E: a scripted, dependency-light harness (see `Notes`) rather than a full
  browser stack in CI.

## Notes

- No new runtime dependency beyond `maud` (workspace addition) and the vendored
  HTMX asset. Everything else reuses existing crates.
- `Agents.md` and `README.md` are updated in this change (new crate + UI docs).
- The `web-ui` capability spec is the single source of truth for the whole
  frontend; each follow-up page change appends `## ADDED Requirements` to it.
