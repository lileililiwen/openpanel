# Proposal: Wire the forge-publish-provider to a real stdio transport

## Why

The `forge-publish-provider` capability (0.1.0) is implemented as a library:
`forge_publish_provider/protocol.rs` codecs JSON-RPC frames,
`facade.rs` executes Preflight/Deploy/Verify/Rollback through the existing
`ApplicationDeliveryService` and `DeploymentAdapterService`, and
`lifecycle.rs` models the enable toggle. Nothing reads stdin. Forge cannot
call `forge.publish` today, which blocks the publish tail of the workspace
toolchain (root `TOOLCHAIN_ROADMAP.md` B5). Reuse: this change adds only the
transport, operator binding, crate-root error-frame exports, and CLI wiring
around the modules named above — no duplicated lifecycle.

## What Changes

- New `openpanel-cli` subcommand `forge-provider` running a
  newline-delimited stdio JSON-RPC loop over the existing
  `JsonRpcCodec` and `ForgePublishProvider` facade.
- Operator caller binding: `OPENPANEL_FORGE_PUBLISH_OPERATOR` names a real
  user loaded through `identity/repo.rs`; the facade's `require_operator`
  (Owner|Admin) remains the sole role gate; fail-fast startup otherwise.
- Expose `decode_envelope`, `encode_error`, and envelope types from the
  `openpanel-app` crate root so callers can emit JSON-RPC error frames.
- Lifecycle status transitions (`start_provider`/`mark_provider_failed`/
  `stop_provider`) driven by the loop; refuse cleanly when the enable env
  is unset, honoring the archived independent-lifecycle requirement.
- Composition seam: production delivery state does not exist yet (the
  delivery services are constructed only in tests; no `BuildAdapter`/
  `RuntimeAdapter`/`DeploymentAdapter` impl ships outside `cfg(test)`).
  v1 composes them with explicit fail-closed `Unavailable` adapters inside
  this module and answers publish requests with the contract's failure
  status plus audit trail. Real docker-backed adapters are the separately
  owned follow-up package below, not a hidden stub.

## Package map (split assessment)

| Package | Single outcome | Owner | Boundary/contract | Depends on | Independent oracle |
|---|---|---|---|---|---|
| `forge-publish-provider-stdio` (this change) | Forge can exchange frames with a running provider process | openpanel-cli + openpanel-app | stdio loop, operator env, crate-root exports | archived `forge-publish-provider` spec | E2E CLI frame-exchange tests |
| `application-delivery-docker-adapters` (future) | publish actually builds/deploys via OpenPanel docker services | openpanel-app | `BuildAdapter`/`RuntimeAdapter`/`DeploymentAdapter` impls | this transport | delivery integration tests |
| Forge consumer (roadmap B5) | `forge publish` invokes the subcommand | forge | `forge-publish-provider/0.1.0` | both above | forge-side e2e |

Transport and adapters have different lifecycles and oracles; forcing one
umbrella change would hide the composition gap. Keeping v1 transport
shippable is the smallest independently verifiable unit.

## Capabilities

- **Modified:** `forge-publish-provider` (adds stdio transport, operator
  caller binding, error-frame exports; keeps all 0.1.0 requirements).

## Non-goals

- No real build/deploy execution (follow-up package), no SQLite delivery
  state migration, no HTTP transport, no multi-caller sessions, no changes
  to `ApplicationDeliveryService`/`DeploymentAdapterService` semantics, no
  forge repository edits, no web UI surface.
