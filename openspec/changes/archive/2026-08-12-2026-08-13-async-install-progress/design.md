# Design

## Task model

`SoftwareCenterService` owns two new pieces of state:

- `artifact_tasks: Mutex<HashMap<Uuid, InstallTaskProgress>>` — the
  live registry. Tasks are never removed, so a completed or failed
  task can still be rendered by the storefront and the fragment
  endpoint without a 404 race.
- `artifact_slot: Arc<Semaphore> (1)` — serializes the actual download
  and placement so only one artifact install writes into
  `/var/lib/openpanel/webapps` at a time. The single slot matches the
  existing package-transaction lock (`software_install_lock`); a task
  that cannot acquire the permit stays `queued`.

`InstallTaskProgress` carries: `id`, `entry_id`, `entry_name`,
`state`, `step`, `percent`, `bytes_downloaded`, `bytes_total`,
`detail` (terminal errors only), and `created_at`. `InstallTaskState`
is `Queued | Downloading | Placing | Installed | Failed` with a
machine label (`queued`/`downloading`/`placing`/`installed`/`failed`)
used in the badge. Both types derive `Serialize` but are only rendered
through the fragment, never exposed as JSON to a browser surface.

## Request flow

`start_artifact_install(actor, role, entry_id)`:

1. `owner(role)` — reject non-Owners before any work.
2. `ensure_seed_materialized()` — first boot seeds the store.
3. Look up the entry; require `EntryKind::Web`; pick the latest
   version and its pinned artifact.
4. Gate: when `require_verified_digests` and `pin.sha256` is the
   placeholder sentinel, return the typed `Invalid` error **before**
   inserting a task. This preserves the fail-closed behavior and is
   exactly the same check the old synchronous path ran, so the
   placeholder-disabled-button test keeps passing.
5. Insert a `Queued` task and return its id immediately.
6. Spawn a `tokio::spawn` background task that acquires the slot,
   then calls the shared `run_artifact_install` pipeline writing each
   progress callback into the registry, and finally marks the task
   `Installed` (on success) or `Failed` (on error) *after* the audit
   was recorded.

The old synchronous `install_artifact` is preserved and delegates to
the same `run_artifact_install` with a no-op progress callback, so the
two paths cannot drift. Failures never record an audit event; the
storefront surfaces them through the terminal fragment instead.

## Shared pipeline

`run_artifact_install(actor, role, entry_id, on_progress)` re-verifies
owner + Web + pin, then:

- `on_progress(Downloading, "Downloading", 0, 0, None)`
- `artifact_fetcher.fetch_progressed(url, |done, total| …)` — the
  callback computes `percent = done.checked_mul(100)? / total`, capped
  at 98 (`percent.min(98)`), so download can never mask the `Placing`
  step. `total` is `None` on chunked responses.
- create the webapps parent + root directories
- `on_progress(Placing, "Placing files", 98, bytes, Some(bytes))`
- `place_artifact_with_gate(pin, bytes, root, require_verified)`
- capture `current_platform()`, record the
  `SoftwareArtifactInstalled` audit event with
  entry/version/archive_type/source_url/digest/digest_verified/bytes/
  destination/platform
- return `ArtifactInstallResult`

The task wrapper maps `Ok` → `Installed`/100 and
`Err(e)` → `Failed`/100 with `detail = Some(e.to_string())`.

## Streaming fetch

`ArtifactFetcher` gains a required method with a default impl:

```rust
async fn fetch_progressed(
    &self,
    url: &str,
    on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
) -> Result<Vec<u8>, SoftwareCenterError>;
```

The default buffers the whole body then calls
`on_progress(len, Some(len))`. `ReqwestArtifactFetcher` overrides it:
it reuses the same TLS-only, redirect-rejecting client, checks the
status, and streams `bytes_stream()` chunks, calling `on_progress`
per chunk with `done` and the `Content-Length` (parsed once from the
headers) as `total`. The file-size ceiling (1 MiB for catalog
manifests, the web-artifact limit) is applied while streaming so an
oversized upstream cannot be buffered indefinitely.

## Fragment

`progress_fragment(task)` renders a `.task-progress` block:

- a `.badge` with `task.state.as_str()`
- `.task-progress__step` (the label) and `.task-progress__percent`
  and, when `bytes_total` is known, `.task-progress__bytes`
  (`done / total bytes`)
- a `.progress` bar with `role="progressbar"` and
  `aria-valuenow/min/max`, filled via inline `style="width: N%"`
- `.progress__bar--ok` when `Installed`, `.progress__bar--error` when
  `Failed`
- `task.detail` as `.task-progress__error` when present
- when non-terminal, a `<div hx-get="/software/jobs/{id}/progress"
  hx-trigger="every 1s" hx-swap="outerHTML">` driver that keeps the
  fragment polling; a terminal fragment omits it so htmx stops
  polling.

The route handler `task_progress_fragment` looks the task up by `id`,
returns `404` when absent, and otherwise renders the fragment through
the authed shell response (Owner-only via `WebUser`, also CSRF-free by
design: it is a `GET` of public progress, matching the existing
`/software/jobs/{id}/progress`-style fragments... note the route is
grouped with the software job routes and requires an Owner session).

## Storefront

The `/software` handler reads `software_center.artifact_tasks()`,
sorts newest-first, and when non-empty renders an "Install tasks"
section (up to 8) above the catalog grid, each task as its own
live fragment. The install page response embeds the fragment for the
task the POST just created, so the whole flow — click, page, bar,
polling, terminal badge — uses the same fragment code.

## Tests

- App-level `start_artifact_install_runs_in_background_and_reports_progress`:
  starts a task, waits for terminal state, asserts the task passed
  through the expected download/place states, and asserts the audit
  event records `digest_verified: false` for the adminer placeholder.
- App-level
  `start_artifact_install_refuses_placeholder_when_gate_is_on`:
  the gate-on service fails before queueing and `artifact_tasks()`
  is empty.
- Integration: the adminer and phpMyAdmin install tests assert the
  progress page, extract the task id, poll the fragment until
  terminal, and then assert the placed file and the storefront
  "Last install" badge. `TestServer` was already using the in-process
  `StagedArtifactFetcher`; it now also wires fresh per-request
  fetchers so a task's streaming call reports the staged bytes, and
  `fetched_artifacts()` / `webapps_root()` remain the assertion hooks.