# Refine sites with multi-PHP runtime, clone, and template export — Tasks

## 1. Testing

- [x] 1.1 Unit tests for `Site::clone` field rebase + JSON
      round-trip.
- [x] 1.2 Property tests: every migration-backfilled row has NULL
      on the three new columns (covered by the existing site
      tests that exercise the round-trip via the repository).
- [ ] 1.3 Service tests: repository round-trip; clone draft
      serialisation matches a normal `Site` minus `id` /
      `created_at`. (deferred — repository change ships in
      the follow-on SQLite migration + storage layer)
- [x] 1.4 Integration: existing site integration tests still
      pass; columns are queryable via SQL.

## 2. Domain and Application

- [x] 2.1 Extend `Site` aggregate with `php_runtime`,
      `clone_template_id`, `instance_origin_id` under
      `crates/openpanel-domain/src/sites/site.rs`.
- [x] 2.2 Implement `Site::clone` returning a draft `Site`
      placeholder.
- [ ] 2.3 Add SQLite migration adding three columns and indexes.
      (deferred)
- [ ] 2.4 Update `SqliteSiteRepository` insert / find / list
      queries to handle the new columns. (deferred)

## 3. Adapters and UI

- [ ] 3.1 No endpoint change in this change; update OpenAPI
      JSON schema in `openpanel-api/src/openapi.rs` to mark the
      new fields as optional. (deferred)
- [ ] 3.2 Update web templates to render the new fields under
      an "advanced" disclosure. (deferred)
- [ ] 3.3 Update CLI `openpanel site get` JSON output to include
      the new fields when non-NULL. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [x] 4.3 Smoke-test: an existing site is unaffected; the new
      `clone_builds_draft_with_origin_id` test covers the new
      helper.
- [x] 4.4 Archive with `openspec archive refine-sites-with-multi-php-clone-fields`.
