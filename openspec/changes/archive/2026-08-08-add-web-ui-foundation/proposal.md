# Add Web UI — Foundation

## Why

OpenPanel has a complete HTTP API (`/api/v1/identity|sites|databases|files|ssl|monitoring`)
and a CLI, but no browser UI. Every other panel in its class — cPanel, baota,
CloudPanel — is bought on its web dashboard. A CLI is a deal-breaker for the
audience this panel targets: a small-VPS operator who wants to click "create
site", watch the disk gauge, and see failed logins, without SSH.

The existing `web/` skeleton (empty dirs, no `package.json`) signals intent but
no design. This change makes the **architectural decision** the rest of the
frontend hangs on:

- **Pure Rust, zero Node toolchain.** No npm, no Vite, no React, no WASM build
  step. The frontend is server-rendered HTML from `maud` templates inside the
  existing Rust workspace, served by the same axum server, and shipped in the
  same static binary. This keeps the project's "single static binary, no script
  engine at runtime" story literally true.
- **HTMX for interactivity.** The one JS file (`htmx.min.js`) is vendored and
  embedded in the binary; no application JavaScript is written. Interactive
  behavior (navigation, forms, filtering) is HTML-attribute driven against the
  server.
- **Session auth reused.** The API already issues `openpanel_session` cookies.
  The web layer authenticates with the same session middleware and cookies — no
  second auth system.

This change delivers the **foundation**: the shell layout, login/logout, auth
gating, static asset serving, and the web router mounting. Resource pages
(dashboard, sites, databases, files, ssl, monitoring, users) land one per
follow-up OpenSpec change, each extending the same `web-ui` capability spec.

## What Changes

- New crate `crates/openpanel-web/` (HTML adapter layer):
  - `layout.rs` — shared shell: sidebar nav, topbar with user menu + logout,
    HTMX navigation (hx-boost), CSRF token in every `<form>`.
  - `login.rs` — `GET /login` page and `POST /login` handler that calls the
    same `IdentityService::login` as the API and sets the `openpanel_session`
    cookie; `POST /logout` invalidates the session.
  - `router.rs` — builds the web sub-router, mounted at `/` beside the
    existing `/api/v1` nest.
  - `assets.rs` — serves `htmx.min.js` and the app stylesheet from embedded
    bytes (`include_bytes!`), plus a small hand-written CSS file.
  - `csrf.rs` — per-session CSRF token, rendered into every form and validated
    on state-changing POSTs.
- New crate `crates/openpanel-web` is added to the workspace; the composition
  root (`openpanel-cli/src/handlers.rs::serve`) builds the web router and merges
  it with `build_router`.
- Web auth middleware: a `WebUser` extractor on top of the existing session
  middleware; unauthenticated page requests redirect to `/login` instead of
  returning 401 JSON.
- Session resolution: the web layer consumes the same `SESSION_COOKIE` cookie
  (`openpanel_session`) the API login sets.
- Tests: unit tests for layout/login rendering and CSRF; integration tests for
  the login/logout/gating flow; E2E bootstrap.

## Non-Goals

- Any resource page (dashboard, sites, databases, files, ssl, monitoring,
  users) — each is its own follow-up change that builds on this shell.
- A SPA / client-side framework, JS build step, or Node toolchain — rejected by
  the pure-Rust decision.
- A new auth system — the web reuses identity sessions and cookies.
- Dark mode / theming system — a single clean stylesheet ships here; theme
  tokens are a follow-up.
- i18n / localization.
- Responsive/mobile layouts beyond a usable sidebar+content split.
- Real browser E2E in CI (see Notes) — a scripted harness is provided.

## Capabilities

### New Capabilities

- `web-ui`: browser UI served by the panel — shell, login, session auth,
  static assets, CSRF. Resource pages extend this capability in follow-ups.
