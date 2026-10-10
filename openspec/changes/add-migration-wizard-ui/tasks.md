# Tasks: add-migration-wizard-ui

## 1. Testing

- [ ] Unit: `migrate::upload` rejects oversized bundles with an
      inline `form-error` and stores nothing (input: 1 byte over
      the cap; assert 413 + error text, no upload id minted).
- [ ] Unit: `migrate::sniff_view` maps each `DriverKind`
      (`cpanel-pkgacct`, `cpanel-legacy-backup`,
      `baota-backup`, `tar-with-json-manifest`) to its label and
      maps `None` to `UnknownSource` (assert all five).
- [ ] Unit: `migrate::policy_view` disables the commit button
      for `Block` and `RequireExplicit` while conflicts are
      present, and enables it for `Skip` / `Replace` (assert
      per-policy button state with one conflict present).
- [ ] Unit: `migrate::run_view` renders the `PartiallyCompleted`
      banner with verifier evidence and disables all three
      cutover buttons when any verifier fails (assert banner +
      three disabled buttons; assert buttons enabled on full
      success).
- [ ] Unit: `migrate::cutover_view` renders preflight failure
      with the imported source left isolated (assert refusal
      text, no success marker).
- [ ] Integration: `GET /migrate` as Owner renders the upload
      form with CSRF token; unauthenticated returns 302 to
      `/login` (`migrate_wizard_upload_form`).
- [ ] Integration: `POST /migrate/preview` with a Baota fixture
      bundle lists planned resources + warnings with no state
      change; hint mismatch returns `UnknownSource`
      (`migrate_wizard_preview_plan`).
- [ ] Integration: `POST /migrate/run` with `ConflictPolicy::Block`
      and a conflicting plan refuses the commit
      (`migrate_wizard_block_on_conflict`).
- [ ] Integration: `POST /migrate/run` with a failing verifier
      records `partially_completed` and
      `POST /migrate/cutover/dns` is refused
      (`migrate_wizard_partial_refuses_cutover`).
- [ ] Integration: `POST /migrate/cutover/{dns,mail,ssl}` each
      require Owner role and valid CSRF; User role gets 403,
      bad CSRF gets 403 (`migrate_wizard_cutover_gated`).
- [ ] Integration: registry/route agreement —
      `capability_registry::find_by_route("/migrate")` resolves
      and the static router-mount guard covers `/migrate`
      (`migrate_wizard_discoverable`).
- [ ] Property: `prop_migrate_policy_never_destructive_on_preview`
      — preview with any policy over arbitrary fixture bundles
      performs zero mutations (plan-only).
- [ ] Property: `prop_migrate_partial_never_auto_cutover` — any
      run with ≥1 failing verifier never enables cutover.

## 2. Implementation

- [ ] Web module `crates/openpanel-web/src/migrate.rs`
      (upload/sniff/preview/confirm/run/cutover handlers +
      maud templates, shared tokens only, no new CSS).
- [ ] Router mounting: `/migrate*` routes in `router.rs` +
      `REGISTRY` entry + `NAV_SECTIONS` Operations item +
      `CapabilitySet::shipped()` coverage.
- [ ] Styling/a11y pass: `form` classes, `<label>` per input,
      `:focus-visible` ring, single `<h1>`, three-breakpoint
      smoke (360/768/1280), `scan-literal` +
      `class-coverage` clean.
- [ ] Strict validation: `openspec validate
      add-migration-wizard-ui --strict --no-interactive` green.
