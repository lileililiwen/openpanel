# Proposal: Restore `make check` by clearing file-length and UI-contract violations

## Why

`make check` was red before this change, for reasons unrelated to any
in-flight feature work. Three source files exceeded the `hard_limit` of
1000 declared in `cargo-lint-extra.toml`, and none of them appears in that
file's `exclude` baseline, so the `file-length` gate failed on every run:

- `crates/openpanel-app/src/sites/nginx.rs` — 1023 lines
- `crates/openpanel-web/src/dashboard.rs` — 1051 lines
- `crates/openpanel-core/src/audit/mod.rs` — 1608 lines

The audit module carried three further latent defects: its `strings.rs`
helper (240 lines) duplicated concerns that belong with the types they
format; `cargo check --all-targets` did not compile because four app-layer
test doubles implemented only `record` and `recent` and omitted the
`query` method that `AuditService` already required; and `render_event_row`
used `unwrap()`, which `openspec/config.yaml` forbids in production code.

Finally, the `/audit` filter form violated the `web-ui-styling` contract:
it declared the ad-hoc class `audit-filters` and rendered six visible
controls with no paired `<label>`. That regression entered with the
committed `fe36170` (2026-08-29), twelve days after
`tests/integration/web_ui_styling.rs` (61170ab, 2026-08-17) began
enforcing the contract.

## What

Decompose each oversized module along the boundaries already present in
it, into `<name>/mod.rs` plus focused submodules, with unit tests moved to
a sibling `tests.rs`. Preserve every public module path with `pub use`
re-exports so the split is behaviour-preserving and callers are untouched
apart from `cargo fmt` re-wrapping.

Implement the missing `query` method on the four test doubles, remove the
`unwrap()`, and bring the `/audit` filter form into the global form-class
vocabulary with a paired label on every visible control.

## Capabilities

### Modified Capabilities

- `audit-activity`: audit core is decomposed into `action`, `cursor`,
  `redaction`, and `sqlite` submodules behind an unchanged `audit` facade.
- `sites`: the nginx renderer moves to `sites/nginx/` with tests
  separated.
- `operations-dashboard`: the dashboard view moves to `web/dashboard/`
  with tests separated.
- `web-ui-styling`: the `/audit` form obeys the global class vocabulary
  and the paired-label rule.

## Non-goals

- No change to any existing requirement text. The deltas only ADD
  requirements that pin the new module layout and bring filter forms
  explicitly into the styling contract, so nothing already specified is
  weakened or reworded.
- No behaviour change to nginx rendering, dashboard composition, or audit
  query semantics; public APIs are unchanged by construction.
- No change to the `file-length` thresholds or to the `exclude` baseline
  in `cargo-lint-extra.toml`; the violations are fixed, not suppressed.
- No refactor of the files still listed as `TBD` in that baseline.
