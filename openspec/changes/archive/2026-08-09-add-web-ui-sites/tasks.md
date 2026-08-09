# Tasks: Add Web UI — Sites

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> Tests go in `#[cfg(test)] mod tests` (unit) and
> `tests/integration/web_ui.rs` (HTTP).

## 1. Testing — Sites pages

- [x] 1.1 Unit test in `crates/openpanel-web/src/sites.rs`: the list
      renders one row per site with domain, status, owner, and PHP
      version.
- [x] 1.2 Unit test: create form renders domain, aliases, owner, PHP
      toggle/version, and document-root fields.
- [x] 1.3 Unit test: detail page renders domain, aliases, document
      root, PHP settings, status, and files/SSL links.
- [x] 1.4 Unit test: inline error region renders a duplicate-domain
      message with the offending domain.
- [x] 1.5 Integration test: authenticated `GET /sites` returns the
      sites table (or empty state).
- [x] 1.6 Integration test: `POST /sites` with a valid form creates the
      site (visible in `GET /sites`) and CSRF validates.
- [x] 1.7 Integration test: duplicate-domain create shows the inline
      error and creates nothing.
- [x] 1.8 Integration test: enable/disable flips the site status.
- [x] 1.9 Integration test: delete requires confirmation and removes
      the site from the list.
- [x] 1.10 Integration test: a non-owner sees only their sites and no
      create/delete actions.
- [x] 1.11 Integration test: unauthenticated `GET /sites` redirects to
      `/login`.

## 2. Implementation — Sites pages

- [x] 2.1 Create `crates/openpanel-web/src/sites.rs` with list, new,
      detail, enable/disable, and delete handlers wired into the web
      router, all behind `WebUser` + CSRF.
- [x] 2.2 Implement the create flow (service call, inline errors,
      HTMX list swap).
- [x] 2.3 Implement the detail page with files/SSL links.
- [x] 2.4 Implement enable/disable + delete-with-confirmation HTMX
      actions.

## 3. Validation

- [x] 3.1 `cargo test --workspace` passes.
- [x] 3.2 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all -- --check` pass.
- [x] 3.3 Manual smoke: create a site via the UI, enable/disable it,
      and delete it. — covered by integration tests
      `web_sites_create_valid_form`, `web_sites_enable_disable_flips_status`,
      and `web_sites_delete_removes_site` in
      `tests/integration/web_ui.rs`, which exercise the full create →
      list → toggle → delete flow through the router.
- [x] 3.4 Commit + archive via OpenSpec.
