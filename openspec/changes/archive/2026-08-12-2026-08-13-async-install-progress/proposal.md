## Why

`real-installs` made the Install button actually download and place
artifacts, but the HTTP request blocked the browser for the whole
download-and-place cycle. On a slow upstream a Web install could pin
a tab for minutes with nothing on screen but a blank page, then dump
a success page with no sense of what happened. Baota (宝塔), cPanel,
and Plesk all model installs as long-running tasks: the click returns
instantly with a queued job and the storefront polls live progress
until the task is terminal. For an operator-driven control panel that
is the expected UX, and it is what this change ships.

## What Changes

* **Install is a background task, not a blocking request.** Clicking
  Install on a Web entry POSTs to `/software/components/{id}/install`.
  The handler validates the entry, the Web-only check, and the
  verified-digest gate **upfront** (fail fast, no task is queued for an
  invalid request), then inserts a `queued` task and returns the
  install page immediately. A spawned background task runs the shared
  download → place → audit pipeline and publishes progress into an
  in-memory `InstallTaskProgress` registry.
* **Real download progress.** `ArtifactFetcher` gains
  `fetch_progressed(url, on_progress)`. The `ReqwestArtifactFetcher`
  streams the response chunk-by-chunk and reports bytes received vs
  `Content-Length`; the default implementation reports the full body
  as one step. The pipeline computes a 0..98 percent from the byte
  counts, pins 98% for the `Placing files` step, and 100% on success.
* **Task lifecycle states.** `InstallTaskState` models
  `queued → downloading → placing → installed | failed`. A
  bounded semaphore (`artifact_slot`) serializes concurrent artifact
  installs; a task that waits on the slot stays `queued`.
* **Live progress fragment.** A new `GET /software/jobs/{id}/progress`
  Owner-only route returns an htmx fragment: a badge, the step label,
  the percent, bytes (`done / total` when `Content-Length` is
  advertised), the bar, and any terminal error. While the task is not
  terminal the fragment carries `hx-get` + `hx-trigger="every 1s"`
  `hx-swap="outerHTML"`, so the browser keeps polling; a terminal
  fragment drops the attributes and polling stops. Unknown ids return
  `404`.
* **Install page and storefront task list use the fragment.** The
  install response embeds the fragment and self-refreshes; the
  storefront renders an "Install tasks" panel with the most recent 8
  tasks, each as a live fragment.
* **Audit only on success and only once.** The background task records
  the `SoftwareArtifactInstalled` audit event after placement succeeds;
  failures record no audit (the storefront surfaces the terminal
  failure through the task fragment instead).
* **Shared synchronous pipeline.** `install_artifact` and
  `start_artifact_install` share one `run_artifact_install` pipeline so
  the two paths cannot drift.

## Capabilities

### Modified Capabilities

- `software-center`: the existing capability gains the background
  install task model (`InstallTaskState`, `InstallTaskProgress`), the
  streaming `fetch_progressed` fetcher, the `start_artifact_install`
  service entry point, the `/software/jobs/{id}/progress` live
  fragment, and the progress-bar rendering on the install page and the
  storefront task list.

## Impact

Adds `InstallTaskState` and `InstallTaskProgress` to
`SoftwareCenterService`, an `artifact_tasks` registry and
`artifact_slot` semaphore on the service, the `fetch_progressed`
method on `ArtifactFetcher` (a new required method with a default
impl so existing fetchers keep compiling), the
`GET /software/jobs/{id}/progress` route, an "Install tasks"
storefront section, and `.task-progress` / `.progress` CSS. The
install integration tests now poll the terminal task state instead
of asserting the synchronous response. No new external services, no
schema change, no audit/UX regressions behind the existing gates.