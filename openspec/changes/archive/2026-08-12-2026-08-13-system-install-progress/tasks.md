## 1. TDD and Tests

- [x] 1.1 Service-test that `start_execute` returns immediately and
      the background task drives the job through package steps with
      `percent`/`step` and records the `SoftwareChanged` audit exactly
      once. —
      `crates/openpanel-app/tests/software_center.rs`.
- [x] 1.2 Service-test that cancelling during a multi-action
      transaction lands `cancelled` at a package boundary and rolls
      back the applied actions. —
      `crates/openpanel-app/tests/software_center.rs`.
- [x] 1.3 Integration-test that the confirm POST returns the
      running-job page immediately, the fragment is polled until
      terminal, and the Recent jobs panel renders the bar. —
      `tests/integration/software_center.rs`.

## 2. Application Core

- [x] 2.1 Add `SystemJobProgress` and a `system_job_progress` registry
      keyed by job id; add `percent`/`step` to `SoftwareJobView` and
      merge them into `jobs(role)` and `push_job` snapshots. —
      `crates/openpanel-app/src/software_center/mod.rs`.
- [x] 2.2 Extract the transaction body of `execute` into the shared
      `run_execute_job` pipeline with an `on_progress` callback and
      per-action execution, progress reporting, and package-boundary
      cancellation checkpoints. —
      `crates/openpanel-app/src/software_center/mod.rs`.
- [x] 2.3 Add `start_execute` that does the upfront steps
      (owner/reconcile/take-preview/digest-check/job-create/durable
      lock) then spawns `run_execute_job`; keep `execute` synchronous
      for the CLI. —
      `crates/openpanel-app/src/software_center/mod.rs`.

## 3. Surface UI

- [x] 3.1 Re-route the web confirm POST to `start_execute` and render
      a "Running job" page embedding the live progress fragment. —
      `crates/openpanel-web/src/software_center.rs`.
- [x] 3.2 Generalize `GET /software/jobs/{id}/progress` to a union
      lookup of artifact tasks and system jobs; render the shared
      fragment. —
      `crates/openpanel-web/src/software_center.rs` and
      `crates/openpanel-web/src/router.rs`.
- [x] 3.3 Render the Recent jobs panel rows with the live fragment and
      keep the Cancel form. —
      `crates/openpanel-web/src/software_center.rs`.

## 4. Validation and Delivery

- [x] 4.1 `cargo test --workspace` is green. —
      `cargo test --workspace`.
- [x] 4.2 `make fmt` and `make clippy` are clean. —
      `make fmt` and `make clippy`.
- [x] 4.3 Archive via `openspec archive`; commit with a
      Conventional-Commit-style message. —
      `git commit` and the `archive` step.