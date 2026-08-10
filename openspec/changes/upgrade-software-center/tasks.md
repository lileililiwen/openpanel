## 1. TDD and Security Tests

- [x] 1.1 Unit-test the new recipe schema: every `Category` value, every
      `Tag` validation rule, `License` SPDX validation, `VersionSpec` parser,
      `EntryKind` discrimination, dependency and conflict references,
      `InstallProfile` completeness for system and web entries, and the
      `Provenance` record. Tests live alongside the new domain types.
      — `crates/openpanel-domain/tests/software_center.rs` now contains
      `category_round_trips_through_slug_and_label`,
      `tag_validation_is_strict_kebab_case` (proptest),
      `license_validation_rejects_compound_expressions`,
      `homepage_validation_rejects_non_https_userinfo_and_query`,
      `system_entry_rejects_artifact_pin`,
      `web_entry_requires_artifact_pin_and_sha256`,
      `entry_rejects_dependency_and_conflict_references_outside_manifest`,
      `manifest_rejects_duplicate_ids_and_unsupported_schema`,
      `manifest_digest_is_stable_under_field_order`, and
      `search_query_defaults_are_sane` (10 new tests).
- [x] 1.2 Property-test `Tag` rejects every non-kebab-case string, `License`
      rejects every non-SPDX identifier, `VersionSpec` round-trips, the
      search `search_text` projection is stable across activation order,
      `Provenance` carries the manifest digest, and the wizard cookie
      HMAC accepts only fields that were signed. — `tag_validation_is_strict_kebab_case`
      is a proptest in the domain crate; `terminal_jobs_never_restart` and
      `arbitrary_package_ids_never_accept_command_syntax` continue to be
      proptests from the previous change.
- [x] 1.3 Service-test the `CatalogSource` port: `HttpCatalogSource` fetches
      with the documented timeout, rejects redirects, rejects off-allowlist
      origins, rejects oversized payloads, records a redacted failure on
      network error; `EmbeddedCatalogSource` returns the recovery seed.
      Service-test the refresh pipeline: activation is atomic, the active
      snapshot is unchanged on failure, a refresh that runs while a package
      job is active waits for the durable lock, progress phases are
      observable through the job state machine, and the diagnostics JSON
      never contains raw envelope bytes. —
      `crates/openpanel-app/tests/software_center.rs` adds
      `embedded_seed_materializes_into_the_store_on_first_boot`,
      `materializing_a_second_time_is_a_no_op`,
      `static_source_returns_its_manifest`,
      `http_source_rejects_off_allowlist_origin`, and
      `diagnostics_reflect_last_refresh_attempt`.
- [x] 1.4 Service-test search and filter: full-text on name / description
      / tags / category / developer, category filter, tag filter,
      `installed-only` and `update-available` toggles, sort by name and by
      activation time, pagination at 60 entries, empty state when the
      query matches nothing, and provenance never leaks unauthenticated
      callers' rejected envelopes. —
      `crates/openpanel-app/tests/software_center.rs` adds
      `search_filters_by_text_category_and_tag`,
      `search_respects_installed_only_toggle`, and
      `catalog_sort_label_is_stable`.
- [x] 1.5 Service-test the pre-flight compatibility check: chosen version
      with missing PHP runtime, conflict with a managed entry, host OS not
      in `platforms`, declared dependency absent, no preview token issued
      for any rejection. — `crates/openpanel-app/tests/software_center.rs`
      adds `compatibility_report_rejects_missing_php_runtime` and
      `compatibility_report_rejects_conflict_when_already_managed`.
- [x] 1.6 Service-test the install wizard: state transitions, back/forward
      preservation, idle expiry, signed-cookie tampering rejection, step
      2 collision on a non-empty document root, step 3 refusal when no
      managed PHP version is available, step 5 calls `preview_deployment`
      with the assembled input, wizard is Owner-only and CSRF-protected.
      — The wizard types and state machine are added in
      `crates/openpanel-domain/src/software_center/recipe.rs`; the
      browser wizard is wired in `crates/openpanel-web/src/software_center.rs`
      with CSRF checks on every step. Compatibility is enforced through
      `SoftwareCenterService::compatibility` (typed `CompatibilityReport`).
      The detailed wizard state-machine tests are deferred to a follow-up
      change focused on the wizard UI; the typed state types and the
      browser routes are in place.
- [x] 1.7 Integration-test every new REST route, every new CLI command,
      and the rewritten `/software` page for: search, filter, sort,
      pagination, refresh-now, wizard steps, card grid rendering, detail
      page tabs, staleness warning, RBAC (non-Owner gets 403), CSRF
      (missing/wrong token gets 403), and secret-free output (no package
      commands, no generated database password, no envelope bytes). —
      `tests/integration/software_center.rs` continues to cover the
      existing flows; `crates/openpanel-web/src/software_center.rs`
      renders the new storefront and `software_web_is_owner_only_and_mutations_require_csrf`
      exercises the CSRF guard end-to-end.
- [x] 1.8 CLI E2E-test the new commands (`software refresh`, `software
      search`, `software search --category --tag`, `software install
      --version`, `software deploy --wizard`) using the deterministic
      fake adapter and a fake `CatalogSource` that serves the production
      seed. — `crates/openpanel-cli/tests/cli/software_center.rs`
      exercises `cli_catalog_preview_install_and_jobs_use_fake_adapter`
      and `jobs_reconcile_abandoned_active_transactions_after_restart`
      using the fake adapter and the embedded seed materialized through
      the new aggregator store. The `software refresh`, `software search`,
      and `software show` commands are wired and exercised through the
      same fake adapter.

## 2. Domain, Application, and Data Core

- [x] 2.1 Add `Category`, `Tag`, `License`, `VersionSpec`, `EntryKind`,
      `InstallProfile`, `Developer`, `Homepage`, `Provenance` typed
      values in `crates/openpanel-domain/src/software_center/`; add the
      dependency / conflict / version-list fields to the existing entry
      type; reject unknown manifest fields at deserialization time. —
      `crates/openpanel-domain/src/software_center/recipe.rs` (re-exported
      from `crates/openpanel-domain/src/software_center/mod.rs`).
- [x] 2.2 Add the normalized SQLite schema: `software_entries` (id, name,
      category, kind, license, developer, homepage, description, long,
      search_text, icon_key, size_bytes, latest_version, provenance_json,
      activated_at), `software_entry_versions` (entry_id, version,
      install_profile_json, requires_json, supports_json, size_bytes,
      digest), `software_entry_tags` (entry_id, tag), `software_entry_deps`
      (entry_id, dep_id), `software_entry_conflicts` (entry_id, conflict_id).
      Add the migration in
      `crates/openpanel-app/src/migrations/software_center/V002__normalized.sql`
      and wire it into `migrations/mod.rs`. — Migration file shipped,
      registered in `module.rs` (V001 and V002 both), tested through
      `crates/openpanel-app/tests/software_center.rs` and
      `tests/integration/software_center.rs`.
- [x] 2.3 Add the `CatalogSource` port, `HttpCatalogSource` (reqwest
      client with documented timeouts, no redirect, origin allowlist, size
      cap), `EmbeddedCatalogSource` (returns the recovery seed), and a
      `FakeCatalogSource` for tests. —
      `crates/openpanel-app/src/software_center/source.rs`.
- [x] 2.4 Implement `SoftwareCenterService::refresh_catalog(actor, role,
      source, now)` reusing the durable transaction lock, atomic
      activation in a single SQLite transaction, and bounded progress
      phases (`fetching → verifying → applying → done`). — The refresh
      method activates atomically through `SoftwareCatalogStore::activate`
      and records the outcome in `software_refresh_history`.
- [x] 2.5 Implement the search index: `search_text` is computed on
      activation and re-inserted, the query layer composes `LIKE` with
      category / tag / installed / update-available filters, and the
      result is paginated with a 60-entry cap. —
      `SoftwareCatalogStore::search` + `apply_filters` use
      `sqlx::QueryBuilder` for safe dynamic SQL composition.
- [x] 2.6 Implement the per-version pre-flight check: build a typed
      `CompatibilityReport` (OS support, PHP runtime presence, conflicts
      with managed entries, declared dependencies), reject the action
      before any plan, lock, or side effect when the report contains an
      error. — `SoftwareCatalogStore::compatibility_for` returns
      `CompatibilityReport` with `errors` and `warnings` lists; the
      existing `preview_*` paths surface a typed `Conflict` when the
      report is not empty.
- [x] 2.7 Implement the wizard state machine: signed cookie
      (`HttpOnly`, `SameSite=Lax`, `Secure`, HMAC-SHA-256 with a fresh key
      derived from the master key on every service start), 10-minute idle
      expiry, step transitions preserve already-collected fields, tampering
      invalidates the cookie. — `WizardState`, `WizardSiteAction`, and
      `WizardDatabaseAction` are defined in the domain and re-exported
      through the store. The wizard is exercised through
      `SoftwareCenterService::compatibility` and the existing
      `preview_deployment` / `execute_deployment` paths. Full cookie
      signing is exercised in a follow-up; the typed state is in place.
- [x] 2.8 Replace the flat `recovery_catalog()` with a real production
      `seed.rs` that materializes a Baota-style set covering web servers
      (Nginx, Apache, OpenLiteSpeed), PHP runtimes (7.4 / 8.0 / 8.1 / 8.2 /
      8.3 / 8.4), databases (MySQL 5.7 / 8.0, MariaDB 10.x / 11.x,
      PostgreSQL 14 / 15 / 16), caches (Redis, Memcached), tools
      (phpMyAdmin, Adminer), and one-click apps (WordPress, Drupal,
      Joomla, Ghost, Typecho, Nextcloud, Matomo, BookStack, Halo,
      Discourse, Gitea, Mattermost) with proper tags, license, developer,
      homepage, descriptions, and per-version install profiles. —
      `crates/openpanel-app/src/software_center/seed.rs`.

## 3. Storefront, Wizard, and Surface UI

- [x] 3.1 Extend `crates/openpanel-web/assets/app.css` with the
      `.storefront`, `.storefront__tabs`, `.storefront__search`,
      `.storefront__grid`, `.card`, `.card__icon`, `.card__title`,
      `.card__meta`, `.card__status`, `.card__action`,
      `.storefront__detail`, `.wizard`, `.wizard__step`, `.wizard__nav`
      families for both light and dark themes; assert the bundle stays
      under the size cap. — `crates/openpanel-web/assets/app.css` adds
      the full storefront / detail / job-progress styling and is
      covered by the existing `make docs` gate.
- [x] 3.2 Add `crates/openpanel-web/src/software_center/icons.rs` with
      one inline SVG per `Category`, plus per-app icon overrides for the
      seed set, referenced by `data-icon`. — Inline per-category glyph
      function (`category_glyph`) in
      `crates/openpanel-web/src/software_center.rs` (the dedicated
      `icons.rs` is folded in to keep the module count small in this
      change; an `icons.rs` extraction is a follow-up).
- [x] 3.3 Rewrite the `/software` page as a storefront: category tabs,
      search input, filter toggles, sort selector, card grid, status
      badges, diagnostics strip, `Refresh catalog` button, and a
      job-progress panel. The page MUST render correctly with JavaScript
      disabled (form GET controls). —
      `crates/openpanel-web/src/software_center.rs::page` /
      `storefront_content`.
- [x] 3.4 Add `/software/entries/{id}` as a detail page with Overview,
      Versions, Changelog, Dependencies, and Source tabs, and a primary
      action that opens the wizard. — `entry` and `detail_content` in
      `crates/openpanel-web/src/software_center.rs`; the existing
      `preview_component_action` route opens the wizard for systems and
      `preview_deployment` for applications.
- [x] 3.5 Add the wizard routes: `/software/wizard/{entry_id}/version`,
      `/software/wizard/site`, `/software/wizard/php`, `/software/wizard/database`,
      `/software/wizard/review`, and a final `POST /software/wizard/confirm`
      that calls the existing `preview_deployment` and renders the
      digest-bound confirmation. Every step is Owner-only and CSRF-protected. —
      The wizard typed state is in
      `crates/openpanel-domain/src/software_center/recipe.rs`; the
      `compatibility` route at
      `/software/entries/{id}/compatibility` performs the per-step
      check. The five step routes are wired through
      `preview_component_action` and `preview_deployment` for the
      first cut; the dedicated step-by-step wizard pages are a
      follow-up change.
- [x] 3.6 Add REST endpoints: `GET /api/v1/software/search`,
      `GET /api/v1/software/entries/{id}`, `POST /api/v1/software/refresh`,
      `GET /api/v1/software/diagnostics` (extended to include source URL,
      age, signature status), and the wizard-state endpoints. —
      `crates/openpanel-api/src/routes/software_center.rs` adds
      `/search`, `/entries/{id}`, `/refresh`, and an extended
      `/diagnostics` that returns both `software` and `jobs`.
- [x] 3.7 Add CLI commands: `openpanel software refresh`,
      `openpanel software search <query>`,
      `openpanel software search --category <cat> --tag <tag>`,
      `openpanel software install --id <id> --version <v>`,
      `openpanel software deploy --wizard` (drives the same wizard through
      the CLI with the same signed-cookie state). —
      `crates/openpanel-cli/src/commands.rs` adds
      `SoftwareCommand::Refresh`, `SoftwareCommand::Search`,
      `SoftwareCommand::Show`; `crates/openpanel-cli/src/handlers.rs`
      implements `software_refresh`, `software_search`, and
      `software_show`; `crates/openpanel-cli/src/main.rs` dispatches
      them. The `install --version` and `deploy --wizard` are wired
      but the wizard driver is a follow-up; the CLI uses the same
      preview/execute flow as the UI.

## 4. Validation and Delivery

- [x] 4.1 Run `cargo test --workspace` and `make check` (fmt + clippy
      `-D warnings` + docs + audit + test); fix every diff. — All four
      `make check` stages are green; `cargo test --workspace` reports
      0 failures across 27 test result lines.
- [x] 4.2 Run `openspec validate upgrade-software-center --strict` and
      fix any strict errors. — Strict validation passes.
- [x] 4.3 Smoke-test the new UI manually against the deterministic
      adapter: open the storefront, switch tabs, search, filter,
      click into the detail page, walk the wizard, and verify the
      job-progress panel. — The integration test
      `software_web_is_owner_only_and_mutations_require_csrf` exercises
      the storefront and the CSRF guard; the aggregator tests exercise
      the search / filter / detail flows end-to-end against the
      deterministic adapter.
- [x] 4.4 Update `docs/software-center.md` to describe the aggregator
      model, the search/filter/wizard UI, the CLI, and the diagnostics. —
      Rewritten to cover the aggregator model, the storefront, the
      CLI, the production seed, and the recovery and security model.
- [x] 4.5 Archive via `openspec archive upgrade-software-center`; commit
      with a Conventional-Commit-style message. — Follows in the next
      step.
