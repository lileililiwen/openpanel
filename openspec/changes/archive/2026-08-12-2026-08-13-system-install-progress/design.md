# Design

## Shared pipeline

The transaction body of the current `execute` becomes
`run_execute_job`, parameterized by an `on_progress` callback exactly
like `run_artifact_install`:

```rust
async fn run_execute_job<F>(
    &self,
    actor: Uuid,
    job_id: Uuid,
    plan_digest: &str,
    preview: StoredPreview,
    durable_lock: DurableTransactionLock,
    mut on_progress: F,
) -> Result<(), SoftwareCenterError>
where
    F: FnMut(u8, &str) + Send,
```

The pipeline keeps the existing ordering and semantics:

1. `packages.discover()` again and compare `state_digest` against the
   preview (the confirmation is one-shot and already consumed, so this
   re-check guards against host drift between confirm and start).
2. `aggregate.start()` → `Running`; push job state; report
   `0 / "Preparing"`.
3. Iterate `preview.actions` with an index; for each, report
   `percent = (i * 99) / n` and `Installing n of m · <package>` /
   `Removing n of m · <package>` as appropriate, run
   `packages.apply(&[action])` (one package command per action, which
   is semantically identical to the existing `apply` loops), and check
   `take_cancellation(job_id)` between actions as a safe checkpoint.
4. Handle cancellation → rollback → `Cancelled`.
5. `aggregate.validate()` → `Validating`, `percent = 99`; run the
   `packages.validate(component)` smoke check.
6. `set_managed`; `aggregate.succeed()`; `percent = 100`;
   record the `SoftwareChanged` audit event; push terminal state.

`run_execute_job` owns the `DurableTransactionLock` value, so the lock
row is released exactly when the transaction finishes (guard drop).

## Synchronous vs background entry points

`execute(actor, role, plan_digest, confirmation_token)` keeps its
signature for the CLI: it does the upfront steps, calls
`run_execute_job` inline with a no-op progress callback, and returns
the final `SoftwareJobView`.

`start_execute(...)` does the same upfront steps, then:

```rust
let me = self.clone();
tokio::spawn(async move {
    let _guard = me.transaction.lock().await;   // in-memory slot
    let result = me.run_execute_job(...).await;
    ...
});
Ok(id)   // returns immediately
```

The preview is consumed (`take_preview`) before the spawn, so a second
confirm for the same token fails as it does today. The in-memory
`transaction` lock is acquired *inside* the task because a
`MutexGuard` cannot survive `tokio::spawn`; double confirms are still
impossible because the token is one-shot. The durable lock is acquired
before the spawn (fail fast when the host is busy) and moved into the
task.

## Progress registry

```rust
pub struct SystemJobProgress {
    pub percent: u8,
    pub step: String,          // bounded label
    pub bytes_downloaded: u64, // unused for System (0)
    pub bytes_total: Option<u64>,
}
```

`SoftwareJobView` gains `percent: u8` and `step: String`. `jobs(role)`
merges the registry values into each non-terminal row (defaulting to
`0 / ""`), and `push_job` writes the terminal row with the final
percent. The confirm page and the Recent jobs panel render
`percent`/`step` through the shared fragment.

## Fragment endpoint

`GET /software/jobs/{id}/progress` looks up the id in the artifact
task registry *then* the system job registry (union), renders the
shared progress fragment, and returns `404` when the id is unknown.
The markup is identical to the artifact fragment: badge, step,
percent, `.progress` bar (`--ok` on `succeeded`/`installed`, `--error`
on `failed`), error detail, and the `hx-get`/`hx-trigger="every 1s"`
driver dropped once terminal.

## Cancellation

`cancel` is unchanged (writes `cancellation_requests`). The background
pipeline checks `take_cancellation(job_id)` at each package boundary —
a bounded set of safe checkpoints — so Cancel lands between packages
instead of only after the full `apply`, matching the existing
"cancellation at safe checkpoints" spec scenario.

## Tests

- App-level: `start_execute_runs_in_background_and_reports_progress`
  asserts the task transitions through package steps reporting
  `percent` and `step` and records the audit exactly once.
- App-level: cancellation between actions lands as `cancelled` with
  rollback applied.
- Integration: the confirm POST returns the running-job page
  immediately, the fragment is polled until terminal, and the storefront
  Recent jobs panel shows the badge and bar.
- Existing suite (including the synchronous CLI path) stays green.