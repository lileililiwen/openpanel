# Tasks: add-migration-wizard-ui

## 1. Testing

- [x] Unit: `migrate::upload` rejects oversized bundles with an
      inline `form-error` and stores nothing (input: 1 byte over
      the cap; assert 413 + error text, no upload id minted).
      — DONE 2026-10-10: `migrate::tests::upload_rejects_oversized_bundles_without_minting_id`
      (`cargo test -p openpanel-web migrate::` 5/5 green).
- [x] Unit: `migrate::sniff_view` maps each `DriverKind`
      (`cpanel-pkgacct`, `cpanel-legacy-backup`,
      `baota-backup`, `tar-with-json-manifest`) to its label and
      maps `None` to `UnknownSource` (assert all five).
      — DONE 2026-10-10: `sniff_view_maps_every_driver_kind_and_unknown`, green.
- [x] Unit: `migrate::policy_view` disables the commit button
      for `Block` and `RequireExplicit` while conflicts are
      present, and enables it for `Skip` / `Replace` (assert
      per-policy button state with one conflict present).
      — DONE 2026-10-10: `policy_view_disables_commit_for_blocking_policies`, green.
- [x] Unit: `migrate::run_view` renders the `PartiallyCompleted`
      banner with verifier evidence and disables all three
      cutover buttons when any verifier fails (assert banner +
      three disabled buttons; assert buttons enabled on full
      success).
      — DONE 2026-10-10: `run_view_marks_partial_and_gates_cutover_buttons`, green
      (one genuine RED first: banner copy contains "disabled"; assertion
      tightened to `<button type="submit" disabled>` ×3).
- [x] Unit: `migrate::cutover_view` renders preflight failure
      with the imported source left isolated (assert refusal
      text, no success marker).
      — DONE 2026-10-10: `cutover_view_refuses_preflight_failure_without_success_marker`, green.
- [x] Integration: `GET /migrate` as Owner renders the upload
      form with CSRF token; unauthenticated returns 302 to
      `/login` (`migrate_wizard_upload_form`).
      — DONE 2026-10-10: green.
- [x] Integration: `POST /migrate/preview` with a Baota fixture
      bundle lists planned resources + warnings with no state
      change; hint mismatch returns `UnknownSource`
      (`migrate_wizard_preview_plan`).
      — DONE 2026-10-10: green (one genuine RED first: "no commit"
      assertion matched the page's own "nothing is committed" copy;
      assertion narrowed to `Run completed`/`MigrationRunCommitted`).
- [x] Integration: `POST /migrate/run` with `ConflictPolicy::Block`
      and a conflicting plan refuses the commit
      (`migrate_wizard_block_on_conflict`).
      — DONE 2026-10-10: 409 + refusal, green.
- [x] Integration: `POST /migrate/run` with a failing verifier
      records `partially_completed` and
      `POST /migrate/cutover/dns` is refused
      (`migrate_wizard_partial_refuses_cutover`).
      — DONE 2026-10-10: green.
- [x] Integration: `POST /migrate/cutover/{dns,mail,ssl}` each
      require Owner role and valid CSRF; User role gets 403,
      bad CSRF gets 403 (`migrate_wizard_cutover_gated`).
      — DONE 2026-10-10: green (owner success path also asserts
      `Cutover committed` per kind).
- [x] Integration: registry/route agreement —
      `capability_registry::find_by_route("/migrate")` resolves
      and the static router-mount guard covers `/migrate`
      (`migrate_wizard_discoverable`).
      — DONE 2026-10-10: green (nav visible to Owner, hidden for User).
- [x] Property: `prop_migrate_policy_never_destructive_on_preview`
      — preview with any policy over arbitrary fixture bundles
      performs zero mutations (plan-only).
      — DONE 2026-10-10: 100 cases, UI gate agrees with
      `policy_blocks_commit` on every policy × conflict input, green.
- [x] Property: `prop_migrate_partial_never_auto_cutover` — any
      run with ≥1 failing verifier never enables cutover.
      — DONE 2026-10-10: 100 cases, green.

## 2. Implementation

- [x] Web module `crates/openpanel-web/src/migrate.rs`
      (upload/sniff/preview/confirm/run/cutover handlers +
      maud templates, shared tokens only, no new CSS).
      — DONE 2026-10-10: new module + `pub mod migrate` in `lib.rs`;
      `async-trait` added to `openpanel-web` deps (workspace pin, no
      lock change). Reuses `ConflictPolicy`/`MigrationPlan`/
      `MigrationRunStatus`/`DriverKind` predicates, the four real
      `PostImportVerifier`s + `FailingVerifier`, `CutoverService`
      ports, `PLAN_TTL`, `redact_metadata`.
- [x] Router mounting: `/migrate*` routes in `router.rs` +
      `REGISTRY` entry + `NAV_SECTIONS` Operations item +
      `CapabilitySet::shipped()` coverage.
      — DONE 2026-10-10: 6 routes; registry `key: "migrate"`,
      capability `migration`, `min_role: Owner`; nav item agrees
      (icon `package`). Neighbor suites green:
      `capability_registry::` 8/8, `nav_model::` 8/8,
      `capability_navigation` integration 4/4.
- [x] Styling/a11y pass: `form` classes, `<label>` per input,
      `:focus-visible` ring, single `<h1>`, three-breakpoint
      smoke (360/768/1280), `scan-literal` +
      `class-coverage` clean.
      — DONE 2026-10-10 (partial, honest scope): every form uses
      `form`/`form-row`, every visible input is label-wrapped,
      single `<h1>` per page, token classes only
      (`form`/`table`/`btn`/`btn-ghost`/`form-error`/
      `op-empty-state`/`op-error-state`), no inline styles or
      literal colours. Full `scan-literal`/`class-coverage` scripts
      and the 360/768/1280 browser smoke were NOT run (forbidden
      heavy checks for this change; coordinator runs gates).
- [x] Strict validation: `openspec validate
      add-migration-wizard-ui --strict --no-interactive` green.
      — DONE 2026-10-10: `openspec validate add-migration-wizard-ui
      --strict --no-interactive` → "Change 'add-migration-wizard-ui' is valid".
