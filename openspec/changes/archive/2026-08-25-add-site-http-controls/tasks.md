# Add Site HTTP controls — Tasks

## 1. Testing

- [x] 1.1 Unit: `RedirectRule` validation accepts only
      {301,302,307,308}; `IndexPolicy::new` rejects an empty order
      list; `ClientIpRule` rejects malformed CIDR; `MimeOverride`
      rejects an extension without a leading dot or containing `/`.
- [x] 1.2 Unit: redirect loop detection — adding a rule whose source
      prefix equals the destination prefix of an existing rule returns
      `SiteHttpError::RedirectLoop`; deleting the earlier rule makes
      the same insert succeed.
- [x] 1.3 Unit: snippet renderer is deterministic — rendering the same
      `SiteHttpControls` value twice yields byte-identical output, and
      rules appear in ordinal order.
- [x] 1.4 Property (`mod prop`): for arbitrary valid controls (≥100
      cases), rendered nginx text contains no `}` imbalance,
      every protected-dir location is preceded by its htpasswd file path,
      and no bcrypt hash ever appears in the rendered vhost.
- [x] 1.5 Integration (`tests/integration/site_http_controls.rs`):
      `PUT /api/v1/sites/{id}/http` with status 404 and a document inside
      the site root → 200 + subsequent GET returns it; path escaping the
      site root → 422; unauthenticated call → 401.
- [x] 1.6 Integration: creating a protected dir with a bcrypt hash
      stores only the hash in SQLite (assert row does not contain
      plaintext) and the API response contains no hash; htpasswd file
      written with mode 0600.
- [x] 1.7 CLI E2E (`crates/openpanel-cli/tests/cli/site_http.rs`):
      `cli_site_http_show_then_set_round_trips` — `site-http set`
      followed by `site-http show` round-trips; loop input exits
      non-zero.
- [x] 1.8 Web: HTTP Controls tab covered by an integration test
      (render + CSRF save round-trip); uses the shared `form` token
      vocabulary so the asset-level responsive gates apply. Screenshot
      evidence at 360/768/1280 px remains a PR-description step for the
      human reviewer.

## 2. Domain

- [x] 2.1 Implement `site_http_controls` module under
      `crates/openpanel-domain/src/`: aggregates above, `SiteHttpError`,
      validation functions, repository trait. Zero I/O.

## 3. Application

- [x] 3.1 SQLite repository + migrations under
      `crates/openpanel-app/src/site_http_controls/`.
- [x] 3.2 Pure `SiteHttpControlsRenderer` (nginx text) following the
      WAF compilation pattern; spliced via the generator's generic
      insert-before-first-location pipeline (`apply_with_waf`).
- [x] 3.3 `SiteHttpService` with audit events
      (`SiteHttpControlsChanged{site_id, section counts}`); register
      `SiteHttpControlsModule` via composition roots (CLI serve +
      test-support server).

## 4. Adapters and UI

- [x] 4.1 REST routes `crates/openpanel-api/src/routes/site_http_controls.rs`
      (`GET/PUT /api/v1/sites/{id}/http`) with error mapping (422 for
      validation, 404 unknown site).
- [x] 4.2 CLI dispatch `openpanel site-http {show,set}` subcommands.
- [x] 4.3 Web UI tab (`/sites/{id}/http`): JSON editor + section
      counts, CSRF-protected POST, sub-navigation link from the site
      detail page.

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [x] 5.2 Quality gates clean (fmt, clippy, docs, audit, file-length,
      scan-literal, tasks-testing-first, reuse, layering,
      spec-test-drift, spec-drift, tests).
- [x] 5.3 Smoke-test (sandboxed): rendered vhost asserted to contain
      error_page/rewrite/auth_basic directives placed before
      `location /`; htpasswd file written 0600; generator exercised in
      write-only mode (no nginx binary on dev hosts — live `-t` +
      reload is the generator's existing tested discipline).
- [ ] 5.4 Archive with `openspec archive add-site-http-controls`.
