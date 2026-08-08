# Tasks: Add Web UI — Foundation

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP). E2E uses the scripted harness
> described in the design Notes — no browser dependency in CI.

## 1. Testing — Web Layer

- [x] 1.1 Unit test in `crates/openpanel-web/src/layout.rs`: the shell
      renders the sidebar (all expected nav links present) and the
      topbar user label; nav links are safe (all `href` attributes
      present).
- [x] 1.2 Unit test in `crates/openpanel-web/src/layout.rs`: every
      `<form>` rendered by the shell/layout helpers includes a hidden
      CSRF token field.
- [x] 1.3 Unit test in `crates/openpanel-web/src/login.rs`: the login
      page renders the username + password fields and a submit button;
      the failure state renders the error message.
- [x] 1.4 Unit test in `crates/openpanel-web/src/csrf.rs`: token
      generation yields 32 random bytes; `verify` accepts the matching
      token and rejects wrong/missing tokens.
- [x] 1.5 Integration test in `tests/integration/web_ui.rs`:
      `GET /` without a session redirects (`302`) to `/login`.
- [x] 1.6 Integration test: `POST /login` with valid credentials sets
      the `openpanel_session` cookie and redirects to `/`; the shell
      renders the logged-in user's name.
- [x] 1.7 Integration test: `POST /login` with invalid credentials
      returns `401` and shows the error form; no cookie is set.
- [x] 1.8 Integration test: authenticated `POST /logout` invalidates
      the session and redirects to `/login`.
- [x] 1.9 Integration test: a state-changing web POST with a wrong or
      missing CSRF token returns `403` and makes no state change.
- [x] 1.10 Integration test: `GET /assets/htmx.min.js` returns the
      script with `Content-Type: application/javascript`.

## 2. Implementation — New crate `openpanel-web`

- [x] 2.1 Add `crates/openpanel-web` to the workspace (`Cargo.toml`),
      depending on `maud`, `axum`, `tower`, `chrono`, and the existing
      `openpanel-domain` / `openpanel-app` crates.
- [x] 2.2 Create `crates/openpanel-web/src/lib.rs` with the module
      layout (`layout`, `login`, `csrf`, `assets`, `router`) and rustdoc
      on every public item.
- [x] 2.3 Implement `crates/openpanel-web/src/layout.rs`: `Shell`
      builder rendering sidebar + topbar + content region, with
      `hx-boost` on nav links and CSRF token in every form.
- [x] 2.4 Implement `crates/openpanel-web/src/login.rs`:
      `GET /login` renders the form; `POST /login` calls
      `IdentityService::login`, sets the `SESSION_COOKIE` cookie
      (HttpOnly, `SameSite=Lax`, `Max-Age=86400`), and redirects;
      failure re-renders with the error.
- [x] 2.5 Implement `crates/openpanel-web/src/csrf.rs`: per-session
      token generation + `ValidateCsrf` extractor returning `403` on
      mismatch.
- [x] 2.6 Implement `crates/openpanel-web/src/assets.rs`: embed
      `htmx.min.js` + `app.css` via `include_bytes!` and serve under
      `/assets/*` with content-type and cache headers.
- [x] 2.7 Implement `crates/openpanel-web/src/router.rs`: build the web
      `Router` (`/`, `/login`, `/logout`, `/assets/*`) and a `WebUser`
      extractor that redirects unauthenticated page requests to
      `/login`.

## 3. Implementation — Wiring + assets

- [x] 3.1 Vendor `htmx.min.js` (pinned version, with license header) and
      write `app.css` under `crates/openpanel-web/assets/`.
- [x] 3.2 Update `openpanel-cli/src/handlers.rs::serve` to build the web
      router and merge it with `build_router`.
- [x] 3.3 Update `crates/openpanel-test-support/src/server.rs` so
      `TestServer` mounts the web router too.
- [x] 3.4 Ensure the login endpoint shares the API's
      `IdentityService` (one session store / cookie contract).

## 4. Documentation + validation

- [x] 4.1 Update root `README.md` with a "Web UI" section (pure-Rust
      HTMX shell, how to log in).
- [x] 4.2 Update `Agents.md` References with the `web-ui` spec and the
      `openpanel-web` crate.
- [x] 4.3 `cargo test --workspace` passes (all new web tests plus
      existing suites).
- [x] 4.4 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [x] 4.5 Manual smoke: boot the server, log in via the browser form,
      confirm the shell renders and logout works.
- [x] 4.6 Commit + archive via OpenSpec.
