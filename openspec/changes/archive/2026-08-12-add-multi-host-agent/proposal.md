# Add a real multi-host agent

## Why

`openpanel-agent` is currently a 13-line re-export of `cli::serve` —
the same handlers, the same DB, the same host. A panel that manages a
fleet of boxes needs a **dedicated per-host daemon** with a control-
plane protocol, mutual TLS, a narrower API surface, and an explicit
trust relationship with the control panel. Without it, OpenPanel is a
single-host competitor to Baota, not a cPanel/WHM-class fleet manager.
The Rust advantage is sharpest here: one tiny static binary per host
with no PHP runtime, talking to one control plane over mTLS.

## What Changes

- Promote `openpanel-agent` from a CLI re-export to a real binary
  with its own configuration, its own (read-only by default) ports,
  and a control-plane protocol.
- Two new bounded contexts: `agent` (the agent-side capabilities) and
  `fleet` (the control-plane side; aggregation of many agents).
- A `FleetToken` (mTLS client cert + scoped bearer fallback) and an
  `AgentRegistration` record per host with stable host fingerprint.
- Agents expose only idempotent operations: read state, fetch a
  filtered slice of monitoring/logs/audit, run a sandboxed recipe
  from a signed manifest, stream progress, and roll back.
- The control panel aggregates per-agent state under `/fleet` and
  renders it in the web UI.
- All agent traffic is mTLS (rustls); the agent refuses any non-mTLS
  connection.

## Capabilities

### New Capabilities

- `agent`: per-host agent runtime, control-plane protocol, registration.
- `fleet`: control-plane aggregation of many agents.

## Impact

- Domain: `Agent`, `AgentRegistration`, `FleetToken`, `RecipeManifest`.
- App: `AgentService` (read-only state, signed recipe execution),
  `FleetService` (registration, aggregation).
- API/CLI/web: `/api/v1/fleet/*` and `/api/v1/agent/*`, web
  `/fleet` page, `openpanel fleet {list,register,revoke,deregister}`.
- Crypto: rustls mTLS; pinned CA for the control plane; per-agent
  client cert with the host fingerprint as the SAN.
- The current `openpanel-agent` becomes a compatibility shim and is
  deprecated; v0.2 removes it.
