## 1. TDD and Tests

- [x] 1.1 Service-test that `start_artifact_install` returns a task
      id immediately and the background task drives the registry
      through download → place → `installed`, recording the
      `SoftwareArtifactInstalled` audit exactly once with
      `digest_verified: false` for the adminer placeholder. —
      `start_artifact_install_runs_in_background_and_reports_progress`
      in `crates/openpanel-app/tests/software_center.rs`.
- [x] 1.2 Service-test that the gate refuses the placeholder digest
      before any task is queued. —
      `start_artifact_install_refuses_placeholder_when_gate_is_on`
      in `crates/openpanel-app/tests/software_center.rs`.
- [x] 1.3 Integration-test the Web install flow end to end: the
      storefront renders the Install button, the POST returns the
      progress page with a task id, and the test polls the fragment
      until terminal before asserting the placed file and the
      "Last install" badge. Covers the single-file case (adminer)
      and the tar.gz case (phpMyAdmin). —
      `web_install_button_for_adminer_downloads_and_places_the_php_file`
      and `web_install_button_for_a_tar_gz_web_entry_also_lands_on_disk`
      in `tests/integration/software_center.rs`.

## 2. Application Core

- [x] 2.1 Add `InstallTaskState` (`Queued | Downloading | Placing |
      Installed | Failed`) and `InstallTaskProgress` to
      `SoftwareCenterService`, plus the `artifact_tasks` registry and
      the single-slot `artifact_slot` semaphore. —
      `crates/openpanel-app/src/software_center/mod.rs`.
- [x] 2.2 Add `ArtifactFetcher::fetch_progressed` with a default
      impl, and a streaming override on `ReqwestArtifactFetcher`
      that reports (done, total) per chunk from `Content-Length`. —
      `crates/openpanel-app/src/software_center/artifact.rs`.
- [x] 2.3 Add `start_artifact_install` (upfront owner/Web/digest-gate
      checks, queue, spawn) and `run_artifact_install`, the shared
      download → place → audit pipeline that also backs the
      synchronous `install_artifact`. —
      `crates/openpanel-app/src/software_center/mod.rs`.

## 3. Surface UI

- [x] 3.1 Route `GET /software/jobs/{id}/progress` to the live
      fragment handler (Owner-only, `404` for unknown ids). —
      `crates/openpanel-web/src/router.rs`.
- [x] 3.2 Render `progress_fragment` (badge, step, percent, bytes,
      bar, error, 1-second htmx polling until terminal) and embed it
      in the install response and the storefront "Install tasks"
      panel (most recent 8). —
      `crates/openpanel-web/src/software_center.rs`.
- [x] 3.3 Add the `.task-progress` and `.progress` CSS families with
      light/dark themes and the `--ok` / `--error` bar variants. —
      `crates/openpanel-web/assets/app.css`.

## 4. Validation and Delivery

- [x] 4.1 `cargo test --workspace` is green, including the updated
      install integration tests. —
      `cargo test --workspace`.
- [x] 4.2 `make fmt` and `make clippy` are clean (clippy: checked
      division, collapsible `if`, `sort_by_key`, too-many-arguments
      allowances). —
      `make fmt` and `make clippy`.
- [x] 4.3 Archive via `openspec archive`; commit with a
      Conventional-Commit-style message. —
      `git commit` and the `archive` step.