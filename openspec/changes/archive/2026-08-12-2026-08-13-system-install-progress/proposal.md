## Why

The async artifact-install work made Web installs background tasks
with live progress, but System installs (nginx, php, mysql, redis —
the `preview_install` / `execute` flow) still block the HTTP request
for the whole transaction. The confirm POST runs `packages.apply` on
the real package tool (apt/dnf/yum/zypper/pacman/apk) to completion,
so a multi-package install can pin the browser for minutes. The
storefront "Recent jobs" panel lists jobs with a state badge and a
Cancel button, but because the execute POST does not return until the
job is terminal, an Operator never sees a running job with a progress
bar — the exact UX problem we just fixed for Web installs.

Baota shows per-package progress while a System install runs
("installing nginx 2/5"). This change delivers the same for the
System path.

## What Changes

* **The confirm POST becomes a background job.** The web `execute`
  handler is re-routed through a new `start_execute` entry point that
  does the upfront work — Owner check, reconcile, take the one-shot
  preview, verify the host state digest, create the job, and acquire
  the durable transaction lock — then spawns the transaction and
  returns a "Running job" page immediately. The page embeds a live
  progress fragment that polls every second, mirroring the artifact
  install task UX.
* **Shared synchronous/background pipeline.** `execute` (kept for the
  CLI and for callers that want the final `SoftwareJobView`) and the
  new background task share one `run_execute_job` pipeline so the
  paths cannot drift.
* **Per-package progress.** `preview.actions` is executed one
  `PlanAction` at a time (which is exactly what the current
  `apply` loops already do — one package command per action), so each
  package boundary can report `percent = i / n` and a step label
  (`Installing 2 of 5 · nginx`). Progress is capped at 99% during the
  package phase so the `Validating` state stays distinguishable.
* **Live job progress registry.** `SoftwareCenterService` gains a
  `system_job_progress` map keyed by job id; `SoftwareJobView` gains
  `percent` and `step` fields so the storefront "Recent jobs" panel
  can render a bar for non-terminal jobs. Terminal snapshots carry the
  final state (`succeeded`/`failed`/`cancelled`) with the bar at
  100%.
* **Fragment endpoint generalizes.** `GET /software/jobs/{id}/progress`
  serves both artifact tasks and system jobs from one union lookup and
  renders the same markup (badge, step, percent, bar, error detail).
  Non-terminal fragments keep polling (`hx-trigger="every 1s"`),
  terminal fragments drop the attributes.
* **Cancellation lands at package boundaries.** Each package boundary
  is a safe checkpoint: the background task re-checks
  `take_cancellation` between actions, so `Cancel` takes effect
  between packages instead of only after the whole batch.

## Capabilities

### Modified Capabilities

- `software-center`: the existing `Software Jobs, Surfaces, and
  Observability` and `System Component Planning and Lifecycle`
  capabilities gain the background confirmation flow, the per-package
  progress registry, the live job fragment, and cancellation check-
  points at package boundaries.

## Impact

Adds `start_execute` and `run_execute_job` to
`SoftwareCenterService`, a `SystemJobProgress` type and an in-memory
registry keyed by job id, `percent`/`step` on `SoftwareJobView`, a
generalized `/software/jobs/{id}/progress` fragment, and progress-bar
rendering for the confirm page and the Recent jobs panel. The
durable-lock RAII guard is moved into the spawned task so the lock
row stays held for the transaction's lifetime and is released on
drop. The CLI keeps the synchronous `execute`. No schema change, no
new services; the concurrent-package guarantee is unchanged because a
single-slot in-memory lock still serializes package transactions.
Application deployments (`execute_deployment`) and catalog refresh are
explicitly out of scope.