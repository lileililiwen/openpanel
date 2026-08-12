# Spec delta: software-center

## ADDED Requirements

### Requirement: Background artifact install tasks with live progress

A one-click Web install SHALL NOT block the HTTP request for the
download-and-place cycle. `POST /software/components/{id}/install`
SHALL validate the request upfront — Owner-only, Web entry, and the
verified-digest gate — and return the install page immediately with a
`queued` background task. A spawned task SHALL run the shared
download → place → audit pipeline and publish progress to an in-memory
`InstallTaskProgress` registry keyed by task id. Concurrent artifact
installs SHALL be serialized by a single-slot semaphore; a task that
waits on the slot SHALL stay `queued`.

The registry SHALL model the lifecycle
`queued → downloading → placing → installed | failed`, with a
human-readable step label, a 0..100 percent, bytes received, total
bytes when the server advertised `Content-Length`, and a terminal error
detail on failure. The audit `SoftwareArtifactInstalled` event SHALL be
recorded exactly once, and only after placement succeeds.

#### Scenario: Owner clicks Install on adminer

- **WHEN** an Owner clicks Install on the adminer Web entry and the
  verified-digest gate is off
- **THEN** the panel POSTs to `/software/components/adminer/install`,
  inserts a `queued` task, and returns an "Installing Adminer" page
  that embeds the live progress fragment. The background task streams
  the download, reports 0..98% from bytes received vs
  `Content-Length`, reports 98% for the `Placing files` step, lands
  the file at `…/webapps/adminer/4.8.1/adminer-4.8.1-en.php`, records
  the audit event once, and marks the task `installed` at 100%.

#### Scenario: A second install waits on the busy slot

- **WHEN** a first artifact install task is downloading and an Owner
  starts a second one
- **THEN** the second task is inserted as `queued` and remains
  `queued` until the first task releases the slot, at which point it
  transitions to `downloading`.

#### Scenario: Gate refuses the placeholder digest before queueing

- **WHEN** an Owner clicks Install on an entry whose pinned digest is
  the placeholder sentinel and
  `OPENPANEL__SOFTWARE__REQUIRE_VERIFIED_DIGESTS=true`
- **THEN** the POST returns the typed error page, no task is inserted
  into the registry, and `artifact_tasks()` stays empty.

#### Scenario: The pipeline fails during download

- **WHEN** the staged artifact cannot be fetched (unreachable URL,
  non-2xx, oversized response) while the background task runs
- **THEN** the task transitions to `failed` with the bounded error
  detail in `task.detail`, no audit event is recorded, and the
  fragment renders the error text with the error-styled bar.

### Requirement: Live progress fragment endpoint

The panel SHALL expose an Owner-only `GET /software/jobs/{id}/progress`
route that returns an htmx fragment for one artifact install task: a
state badge, the step label, the percent, bytes (`done / total` when
advertised), the progress bar, and any terminal error detail. An
unknown task id SHALL return `404`. While the task is not terminal the
fragment SHALL carry `hx-get` to the same route with
`hx-trigger="every 1s"` and `hx-swap="outerHTML"`; a terminal fragment
SHALL drop those attributes so polling stops. The rendered HTML MUST
NOT contain package commands, raw arguments, or secret material.

#### Scenario: The install page self-refreshes until done

- **WHEN** an Owner lands on the install page and the task is
  `downloading`
- **THEN** the embedded fragment carries the 1-second polling
  attributes, the fragment endpoint returns an updated percent on each
  poll, and once the task reaches `installed` the returned fragment has
  no polling attributes and the browser stops polling.

#### Scenario: Unknown task id

- **WHEN** an Owner requests `/software/jobs/{uuid}/progress` for a
  task id that is not in the registry
- **THEN** the route returns `404` with no body.

#### Scenario: Storefront task list shows recent tasks

- **WHEN** an Owner visits `/software` while install tasks exist
- **THEN** the storefront renders an "Install tasks" panel listing the
  most recent 8 tasks, each rendered as the same live fragment, and
  each non-terminal fragment keeps polling until terminal.

### Requirement: Streaming artifact fetch reports progress

`ArtifactFetcher` SHALL provide `fetch_progressed`, which fetches a
URL and invokes a callback as bytes arrive. The production
`ReqwestArtifactFetcher` SHALL stream the response body and report
(done, total) per chunk, where total is the `Content-Length` when
known; the default implementation SHALL report the full body as a
single step so non-streaming fetchers keep working. The install
pipeline SHALL use `fetch_progressed` and derive the 0..98% percent
from `done * 100 / total` using checked arithmetic, capping download
progress at 98% so the `Placing files` step is always visible.

#### Scenario: Content-Length is advertised

- **WHEN** the artifact response carries a `Content-Length` header
- **THEN** the callback reports the running byte count against the
  total in each chunk, and the fragment renders `done / total bytes`.

#### Scenario: Content-Length is absent

- **WHEN** the artifact response has no `Content-Length` (chunked
  transfer)
- **THEN** the total stays `None`, the percent stays 0 until the body
  completes, and the fragment omits the byte readout instead of
  rendering a `/ 0`.