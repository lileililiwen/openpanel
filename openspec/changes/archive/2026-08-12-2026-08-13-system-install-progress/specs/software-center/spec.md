# Spec delta: software-center

## ADDED Requirements

### Requirement: Background system install confirmation

Confirming a System install, update, or removal SHALL NOT block the
HTTP request for the package transaction. `POST
/software/components/{id}/preview`-driven execution (`execute` —
the confirm POST from the confirmation page) SHALL validate the
Owner, consume the one-shot preview, verify the host state digest,
create the job, and acquire the durable transaction lock upfront,
then return a "Running job" page immediately with a live progress
fragment. The transaction SHALL run in a spawned background task that
executes the shared pipeline and publishes progress until the job is
terminal. The one-shot preview SHALL keep confirming single-use; the
durable lock SHALL be held by the task and released when it ends.
The CLI SHALL keep the synchronous path that returns the terminal
`SoftwareJobView`.

#### Scenario: Confirm an nginx install

- **WHEN** an Owner confirms an nginx install plan
- **THEN** the POST validates the Owner, consumes the preview token,
  acquires the durable lock, and returns a "Running job" page that
  embeds the live progress fragment. The background task runs the
  package transaction, publishes progress (`Installing 1 of 1 ·
  nginx`), validates the service, records the `SoftwareChanged`
  audit event once, and ends the job `succeeded`; the fragment stops
  polling at the terminal state.

#### Scenario: Double-confirm the same token

- **WHEN** an Owner confirms the same plan twice before the first
  transaction finishes
- **THEN** the second confirm fails on the consumed one-shot token
  before any package command runs.

### Requirement: Per-package progress for system transactions

A background system transaction SHALL report progress per package.
The action list SHALL be executed one action at a time (matching the
existing per-package `apply` loops) and SHALL update a live
`SystemJobProgress` record keyed by job id after each action with a
0..100 percent (`i * 99 / n` during the package phase) and a bounded
step label such as `Installing 2 of 5 · nginx`. `SoftwareJobView`
SHALL carry `percent` and `step` so the Recent jobs panel and the
confirm page render a progress bar. The percent SHALL never reach
100 before validation: package phase caps at 99, `Validating` reports
99, terminal states report 100.

#### Scenario: Multi-package plan on an Ubuntu host

- **WHEN** an Owner confirms a plan that installs php-8.3, its
  extensions, and nginx (5 packages)
- **THEN** the fragment advances `Installing 1 of 5` → `Installing 2
  of 5` → … with `percent = i * 99 / 5`, reports `Validating` at 99,
  and ends at `succeeded` / 100.

### Requirement: Live system job fragment

`GET /software/jobs/{id}/progress` SHALL serve both artifact install
tasks and system jobs from a union lookup and return the shared
progress fragment (state badge, step label, percent, progress bar,
bytes when the source advertises them, and terminal error detail).
An unknown id SHALL return `404`. Non-terminal fragments SHALL carry
the 1-second htmx polling attributes; terminal fragments SHALL drop
them. The storefront Recent jobs panel SHALL render each non-terminal
job as the same live fragment with its Cancel form.

#### Scenario: Storefront Recent jobs panel is live

- **WHEN** a background system install is running and an Owner visits
  `/software`
- **THEN** the Recent jobs panel renders the running job with a live
  bar that polls the fragment until the job ends, alongside the Cancel
  action.

#### Scenario: Cancellation lands at a package boundary

- **WHEN** an Owner clicks Cancel while the transaction is running and
  the running package action is still active
- **THEN** the record is marked cancellation-pending and the task
  transitions to `cancelled` at the next package boundary, rolling
  back the actions applied so far.