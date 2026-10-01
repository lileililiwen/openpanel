# Design: forge-publish-provider stdio transport

## Explore & Reuse

`scripts/repo-map.sh` + targeted reads; reuse recorded per Agents.md §8.3:

| Need | Reused exactly | No new duplicate |
|---|---|---|
| frame codec | `forge_publish_provider/protocol.rs`: `JsonRpcCodec::decode_line/encode_line`, `decode_request`, `encode_response`, `encode_error`, `JsonRpcParseError::json_rpc_code` (`Frame` −32700 / `Method` −32601 / `Request` −32602) | transport calls these unchanged; only crate-root visibility added |
| execution | `facade.rs`: `ForgePublishProvider::new/config/handle_request`, `require_operator` role gate, `map_error_to_status` | no second facade, no lifecycle duplication |
| toggle/status | `lifecycle.rs`: `FORGE_PROVIDER_ENABLE_ENV`, `ForgeProviderProcessHandle::{from_environment, is_provider_enabled, start_provider, provider_status, stop_provider, mark_provider_failed}` | loop drives existing handle only |
| user resolution | `identity/repo.rs` + `identity/service.rs` (Owner/Admin roles) | no parallel auth |
| persistence/audit | `handlers.rs::bootstrap_persistence` (Sqlite pool, `SqliteAuditService`), `load_master_key` | same composition path other handlers use |
| CLI shape | `commands.rs` clap `Command` enum + `handlers.rs` one-async-fn-per-action + `main.rs` match arm; `Config::load` (`OPENPANEL_CONFIG`, `OPENPANEL__*`) | `forge-provider` is a new arm, not a new binary |
| E2E harness | `crates/openpanel-cli/tests/cli/` + `common::CliRunner` (spawn with `OPENPANEL__DATABASE__URL`) | new `forge_provider.rs` module in `tests/cli/main.rs` |

## Boundary

- `crates/openpanel-app/src/forge_publish_provider/mod.rs` — add
  `transport.rs` (pure loop fn) + `unavailable.rs` (fail-closed
  `BuildAdapter`/`RuntimeAdapter`/`DeploymentAdapter` markers returning the
  delivery types' unavailable error); re-export
  `decode_envelope, encode_error` + envelope/request/response types.
- `crates/openpanel-app/src/lib.rs:268` — extend the existing re-export list
  accordingly (single line block; no new public surface elsewhere).
- `crates/openpanel-cli/src/commands.rs` — `Command::ForgeProvider` variant
  (no args; everything via env/config).
- `crates/openpanel-cli/src/handlers.rs` — `pub async fn forge_provider(...)`:
  env check → user load → service composition → loop.
- `crates/openpanel-cli/src/main.rs` — match arm.
- `crates/openpanel-cli/tests/cli/forge_provider.rs` (+`mod` registration).
- Must not change: `protocol.rs` wire semantics, `facade.rs` behavior,
  delivery/deployment service traits or their state model, web/api routes,
  governance manifests (no touched pinned specs — `reuse-strict`: new public
  type names `ForgeProviderTransport`, `UnavailableBuildAdapter`,
  `UnavailableRuntimeAdapter`, `UnavailableDeploymentAdapter` are unique
  across crates; verify with `scripts/check-reuse.sh --strict`).

## Contract

- Invocation: `OPENPANEL_FORGE_PUBLISH_PROVIDER=1 openpanel-cli
  forge-provider` with `OPENPANEL_FORGE_PUBLISH_OPERATOR=<username>`.
- Transport loop: read one line from stdin → `decode_line`; parse error ⇒
  `encode_error(code, id=None-or-parsed)` to stdout; wrong method ⇒ −32601;
  envelope present-but-invalid request ⇒ −32602; valid request ⇒
  `handle_request(req, &caller)` ⇒ `encode_response` + `\n` + flush.
  Notifications (id absent) get no response line. EOF ⇒ graceful stop,
  exit 0. Startup with env unset / operator missing / user unknown / role
  below Owner|Admin ⇒ one stderr line + exit 78 (`EX_CONFIG`), no stdin
  reads (keeps archived "independent lifecycle: OpenPanel remains
  operational" scenario true — provider is opt-in, never implicit).
- Wire contract version unchanged: `forge-publish-provider/0.1.0`, identity
  `openpanel-forge-publish-provider`.
- v1 composition: `ApplicationDeliveryService::new(unavailable build,
  unavailable runtime, audit)` + `DeploymentAdapterService::new(vec![],
  audit)`; publish-family requests therefore return the contract's failure
  status (the `ForgeProviderStatus` unavailable/failed variant chosen at
  implementation from the protocol enum — behavior identical from Forge's
  side: refused + audit event). The follow-up adapter package replaces only
  the composition inputs, not this transport.
- Audit: each accepted frame's outcome recorded via existing
  `AuditService` with actor `forge-publish-provider` (facade default).
- Redaction: inherited (facade/protocol guarantee); transport logs no
  request payloads, only ids and statuses.

## Toggle and failure behavior

| Condition | Behavior |
|---|---|
| enable env unset/`0`/invalid | refuse at startup, exit 78, no loop |
| operator env unset | same startup refusal |
| user unknown / role < Admin | startup refusal naming the failure kind, not the hash/secret material |
| stdin garbage line | JSON-RPC error frame, loop continues |
| handler panic safety | loop body wraps `handle_request` so one poisoned request cannot kill the provider; error frame + `mark_provider_failed` then continue (fail-closed per request) |
| stdin EOF | `stop_provider`, exit 0 |
| audit write failure | error frame −32603-style internal via `encode_error` fallback path + `mark_provider_failed` |

## Verification

- `## 1. Testing` artifacts (see tasks): unit per new public fn (loop
  stepping over an in-memory `BufRead` — `transport::run_loop(reader,
  writer)` designed for sync-free testability by taking generics),
  codec-error table test, startup-refusal table test, E2E per CLI
  subcommand via `CliRunner` (valid publish frame ⇒ structured failure
  status response + audit row; garbage frame ⇒ −32700; wrong method ⇒
  −32601; enable-off ⇒ exit 78 with no stdin consumption).
- Coverage map: requirement STD-001 ⇄ loop unit+E2E; STD-002 ⇄ refusal
  tests; STD-003 ⇄ export compile-test (`cargo test -p openpanel-app`
  referencing from another module) + reuse gate.
- Full chain: `make check` (must stay green incl. `tasks-testing-first`,
  `reuse-strict`, `file-length` — `handlers.rs` is 6.9k lines, so the new
  handler goes into `forge_provider.rs` in a `commands`-adjacent module if
  the length gate trips; decide at implementation and record), `make gate`
  (`driftwatchdog gate`), `openspec validate --all --strict
  --no-interactive`.
- Human review gate (Agents.md §8.10): this design.md is the artifact to be
  approved before apply.

## Approved execution details — 2026-10-01

Principal approval provenance: the user's explicit instruction to dispatch and complete the active specs sequentially authorizes apply of this existing package. Parent delegated this package under that directive before this session's implementation. Inherited dirty draft work predates this approval; its tests-first history cannot be established.

Reuse findings refine the planned composition: `DeploymentAdapterService::new(vec![], audit)` already fails closed on unavailable adapters, so no redundant deployment marker is introduced. Build/runtime markers remain. CLI implementation lives in `forge_provider.rs` nested under handlers to preserve bootstrap reuse and file length. Identity lookup directly reuses `SqliteUserRepository` rather than bootstrapping unrelated identity services. Startup matches `Owner | Admin` explicitly (enum ordering is not privilege ordering), rejects disabled users, and returns 78 without stdin reads. It requires a previously initialized identity database.

Transport uses bounded `AsyncBufRead::fill_buf`/`consume` framing, draining only the oversized frame. It dispatches valid notifications without any response, suppresses invalid notification errors, sanitizes codec messages, and propagates writer/reader failures. A shared `ForgeProviderAudit` sink wraps existing facade/delivery audit writes and observes failures without duplicating events; failures and handler unwinding return -32603 with redacted text. `tokio::spawn`/`JoinError` catches asynchronous handler unwinding. The dedicated CLI process installs a payload-free panic hook and avoids the default tracing subscriber, whose stdout output would corrupt the protocol. EOF stops the lifecycle; I/O errors mark it failed. No change to facade/protocol or delivery semantics.

Targeted audit readback demonstrated that core `SqliteAuditService::parse_action` did not recognize existing Forge action variants and silently skipped persisted provider records. Add only those five enum/string mappings; the notification/publish SQLite roundtrip is its red/green oracle. Fresh-process publish against the empty registry returns the existing `invalid_request` status for unknown applications; it is a structured terminal refusal, never simulated deployment.

Audit red-phase diagnostics narrowed the readback defect further: `recent()` had a second, duplicated action decoder, while `query()` uses `parse_action`. Replace the recent decoder with the shared parser; every previous recent mapping is already in the shared parser, and Forge action mappings now apply to both read paths. This removes duplication rather than adding a second Forge table.

Required repeatability run exposed an unrelated real gate flake in `tests/integration/operator_security.rs`: a 1 s wall-clock suppression expiry elapsed during two HTTP requests under load; lengthening only the post-request sleep had not fixed it. Keep real HTTP suppress/queue checks, use 1 h future expiry, drive the existing explicit `OperatorSecurityService::queue(caller, now)` seam at one nanosecond before and exactly at expiry, then confirm reopened state over HTTP. This test-only gate repair preserves production behavior and strengthens its exact expiry oracle without new APIs or a longer sleep.
