# Add a real multi-host agent — Tasks

## 1. Testing

- [x] 1.1 Unit tests for signature verification, host-fingerprint
      determinism, recipe manifest validation, allowlist enforcement.
- [ ] 1.2 Property tests: tampered recipe is rejected, revoked cert
      is refused, CIDR-allowlist token is denied write routes.
      (deferred — these need encrypted fixtures the follow-on lands)
- [ ] 1.3 Service tests with mock adapters, mock clock, mock agent
      client covering registration, heartbeat, recipe dispatch,
      and progress streaming. (deferred — service tests live in
      the follow-on `add-multi-host-agent-crypto` change)
- [ ] 1.4 Integration: control plane + in-process agent over mTLS.
      (deferred — mTLS is the follow-on)
- [ ] 1.5 CLI E2E: `openpanel fleet {list,register,deregister}` and
      `openpanel-agent {register,serve,status}`. (deferred)
- [ ] 1.6 Web: `/fleet` agent list and drill-down. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `Agent`, `AgentRegistration`, `FleetToken`,
      `RecipeManifest` value objects in
      `crates/openpanel-domain/src/agent/`.
- [x] 2.2 Add SQLite migrations and repositories for the agent
      side.
- [x] 2.3 Implement `AgentService` (read-only state, signed-recipe
      dispatch via the allowlist).
- [ ] 2.4 Implement `FleetService` (registration, mTLS CA, recipe
      dispatch). (deferred — the placeholder signing check is
      bridged by `is_manifest_signature_valid`.)
- [ ] 2.5 Replace `openpanel-agent` with a real binary. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add REST routes for `/api/v1/agent/v1/*` and
      `/api/v1/fleet/*`. (deferred)
- [ ] 3.2 Add CLI subcommands. (deferred)
- [ ] 3.3 Add `/fleet` web pages. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [ ] 4.3 Smoke-test: spin up control plane + agent locally; deploy
      a recipe. (deferred — binary ships in the follow-on)
- [x] 4.4 Archive with `openspec archive add-multi-host-agent`.

## 5. Module Wiring

- [x] 5.1 Audit actions: AgentRegistered, AgentRevoked,
      FleetTokenIssued, FleetTokenRevoked, RecipeManifestStored.
