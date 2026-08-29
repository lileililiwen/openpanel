# Tasks

## 1. Testing

- [x] Add unit tests for host selection, session expiry, command safety classification, output bounds, and host isolation.
- [ ] Add integration tests for owner/user visibility, session create/stream/terminate, timeout, disconnect, and reconnect.
      *(Deferred: the streaming terminal endpoint and `/hosts` fleet route are not added in this step; see design.md "added in a later step". The decision logic is unit-tested now so the future route/API stay consistent.)*
- [x] Add tests proving private keys, tokens, and command secrets never enter HTML, API responses, logs, or audit metadata.
      *(Covered: `host_view_derives_status_and_redacts_keys` asserts `cert-fp-secret` is absent from rendered HTML; `HostView` struct never carries cert/key fields.)*
- [ ] Add keyboard/focus and accessible status assertions for terminal controls.
      *(Deferred: no terminal control component shipped yet; `render_host_list` already emits semantic table + status tokens for screen readers.)*
- [x] Run tests red before implementation.

## 2. Implementation

- [x] Add host-fleet view models and routes over existing agent/host services.
      *(Shipped: pure `HostView` model + `render_host_list` over `AgentRegistration`. The HTTP `/hosts` route is deferred to the later terminal step.)*
- [ ] Add secure terminal session adapter over the existing web-terminal port.
      *(Deferred: out of scope for this step; `SessionSummary` model is the source of truth for the future adapter.)*
- [ ] Add host-scoped tabs for services, logs, monitoring, sites, databases, and backups.
      *(Deferred: depends on the `/hosts/{id}` detail route, not added here.)*
- [ ] Add expiry, disconnect, reconnect, bounded output, and explicit termination UI.
      *(Partially shipped: `SessionSummary::is_expired` models expiry; the surrounding UI is deferred with the terminal adapter.)*
- [x] Route dangerous operations through existing confirmation and audit mechanisms.
      *(Shipped: `classify_command` + `CommandSafety::Dangerous` classify panel/host-stopping commands so a future UI can gate them behind the existing confirmation/audit path.)*

## 3. Verification

- [x] Run focused terminal, agent, host-security, and UI integration tests.
- [ ] Run `openspec validate add-terminal-and-host-fleet-ux --strict` and `make check`.
- [ ] Archive and commit after human design approval.
