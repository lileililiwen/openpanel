# Tasks: forge-publish-provider stdio transport

## 1. Testing

- [x] Unit: `transport::run_loop` over `BufReader<&[u8]>` + in-memory writer
  — valid request frame yields exactly one response line + newline;
  notification yields none; EOF terminates cleanly.
- [x] Unit: error table — garbage line ⇒ −32700 frame; wrong method ⇒
  −32601; invalid envelope ⇒ −32602; each continues the loop.
- [x] Subprocess-isolated startup refusals (enable env unset/invalid, operator env unset,
  unknown user, role below Admin) ⇒ exit 78 path, zero stdin reads.
- [x] Unit: handler-poisoning request (unavailable adapters) returns the
  contract failure status and the loop survives.
- [x] Integration: composition with `Unavailable*` adapters +
  `SqliteAuditService` (temp db) ⇒ one audit row per accepted frame.
- [x] E2E per CLI subcommand: `crates/openpanel-cli/tests/cli/
  forge_provider.rs` via `common::CliRunner` — publish frame against
  temp-DB operator user ⇒ structured failure-status response; enable-off
  run ⇒ exit 78. Register module in `tests/cli/main.rs`.
- [x] Redaction: assert stderr/stdout logs never contain payload strings
  from fixtures.

## 2. Implementation

- [x] `forge_publish_provider/transport.rs` — `run_loop` generic reader/
  writer; `unavailable.rs` — fail-closed build/runtime markers; empty deployment registry reuses existing unavailable behavior.
- [x] `mod.rs` + `lib.rs:268` re-exports (`decode_envelope`, `encode_error`,
  envelope types).
- [x] `commands.rs` `Command::ForgeProvider`; `handlers.rs` (or adjacent
  module if `file-length` trips) `forge_provider`: env checks → user load
  via identity repo → bootstrap_persistence → facade composition → status
  transitions → `run_loop(stdin, stdout)`.
- [x] `main.rs` match arm; lifecycle handle drives
  `start_provider`/`stop_provider`/`mark_provider_failed`.

## 3. Regression

- [x] Archived `forge-publish-provider` requirements still hold: facade
  unchanged; independent-lifecycle scenario re-verified (server boots and
  operates with provider env unset; new tests assert no implicit startup).
- [x] Idempotency/rollback and redaction requirements: existing
  `tests/integration/forge_publish_provider.rs` still green unchanged.
- [x] `scripts/check-reuse.sh --strict` (new type names classified if
  needed); `make check` full chain; `make test-gates`/`driftwatch-gate`
  lockstep script.
- [x] Confirm no forge repository or delivery-service semantics touched.

## 4. Verification

- [x] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` (workspace + `-p openpanel-cli`).
- [x] `openspec validate --all --strict --no-interactive`; human approval
  of design.md recorded before apply (Agents.md §8.10).
- [x] `make gate`; task boxes tick only with evidence; verify current-spec pointer and prepare HANDOFF two-commit cadence.

## Evidence — 2026-10-01

- Approval: current user's explicit sequential-active-spec execution directive, delegated by main agent before apply; provenance in design/progress. Inherited dirty source did not contain trustworthy red-phase evidence; this execution introduced contract tests before replacement.
- Testing/implementation: `cargo test -p openpanel-app forge_publish_provider --lib` passed 28/28, including 9 transport/adapter/export/audit tests and 19 unchanged facade/lifecycle/protocol regressions. CLI `cargo test -p openpanel-cli --test cli forge_provider` passed 2/2, including all startup-refusal table cases with stdin held open, Owner/Admin subprocess exchanges, redaction, and exactly one persisted audit row for the accepted publish frame. Startup refusals use isolated subprocess tests rather than process-global environment mutation.
- No HTTP routes or new domain invariants were added. SQLite transport/audit integration is exercised by the unit fixture with fresh TestDb and by CLI subprocess E2E. Public crate-root exports compile from the independent CLI crate and its real process binary.
- Original HEAD had 181 recent-action parser keys; every key remains covered by the shared parser (186 keys including Forge). Notification readback red phase showed the row existed but was skipped; shared-parser reuse makes its regression green. Codec errors, bounded oversized reads, notification dispatch/suppression, internal failures and error Debug/source redaction are covered.
- Strict `openspec validate --all --strict --no-interactive`: 101 passed, zero failed before archive. `scripts/check-reuse.sh --strict` and `git diff --check` pass. Full gate evidence follows when complete.

Bookkeeping after verification follows the mandated two-commit cadence: implementation + all checked tasks + archive first; HANDOFF-only follow-up second, using commit 1's hash. The current-spec pointer is set before implementation; its final successor and commit evidence belong in that follow-up.

- Full serial `make -j1 check` exited 0: 2,100 workspace/all-target tests passed, zero failed, two existing tests ignored; integration 368/368, app 552/552, CLI 48/48 (including provider frames and HTTP `/health` with provider variables removed), domain 679/679, web 225/225. Governance selftests 131/131; lockstep, reuse, layering, docs/catalog, audit and all product contract gates pass. Local optional release-artifact/evidence/browser-live/coverage legs were not enabled; this proves transport and local regression, not a release or production deployment.
- No Forge checkout or delivery-service/facade/protocol semantics changed. Needed core audit readback correction reuses existing parser; generated capability catalog repaired by main agent as necessary gate recovery. Final source review by main found no blocker.

- Required repeatability run uncovered a pre-existing wall-clock race in suppression integration; it is not exempted. Corrected test uses the existing explicit service clock at before/exact expiry and retains HTTP assertions. Focused `cargo test -p openpanel --test integration operator_security_control_plane_suppression_expires_and_reopens` passed 1/1. Full check and identical repeat then completed on that stable fixture.

- Stable fixture verification: `make -j1 check` exited 0 (`/tmp/stdio-make-check-stable.log`); the bound workspace test step reported 2,100 passed, zero failed, two ignored. An independent `make -j1 test` repeat exited 0 with the identical counts (`/tmp/stdio-test-stable-repeat.log`). The full check covered fmt, all-target clippy, docs, strict reuse, test-gates, driftwatch lockstep and the remaining repository gates. `git diff --check` exited 0.
- Canonical `make -j1 gate` exited 0 (`/tmp/stdio-gate.log`) with no blocking failure. The `tests` adapter returned `REVIEW_REQUIRED` at its 60-second budget; the completed bound test command in the serial `make check` and its identical repeat above resolve that pending review under Agents.md section 5. `HANDOFF.md` currently points to this active change; its successor and commit-1 hash will be recorded in the required HANDOFF-only follow-up commit.
