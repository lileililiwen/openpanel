# Add a real multi-host agent — Design

## Topology

```
   ┌─────────── Control Plane ──────────┐
   │ openpanel (single host or HA pair) │
   │   fleet: list agents, aggregate    │
   │   state, dispatch recipes, view    │
   │   progress                         │
   └──────▲────────────▲──────▲─────────┘
          │ mTLS        │ mTLS │
   ┌──────┴───┐  ┌──────┴───┐  ┌── ... ─┐
   │ agent-1  │  │ agent-2  │  │ agent-N│
   │ own DB   │  │ own DB   │  │ own DB │
   │ sysinfo  │  │ sysinfo  │  │ sysinfo│
   └──────────┘  └──────────┘  └────────┘
```

- Each agent owns its own SQLite store and never shares it with
  other agents or the control plane.
- The control plane persists only metadata about each agent
  (`agent_id`, `host_fingerprint`, `last_seen`, `version`).
- The agent never initiates writes back to the control plane's DB.

## Identity

- mTLS is mandatory. The agent presents a client certificate
  generated at registration; the SAN is the agent's stable host
  fingerprint (SHA-256 over `(machine_id, /etc/machine-id, OS release)`).
- The control plane validates the cert against a pinned CA and a
  per-agent certificate revocation list.
- A scoped `FleetToken` (long-lived bearer) is offered as a fallback
  for read-only monitoring where cert distribution is impractical
  (e.g. behind NAT). The token's CIDR allowlist is enforced.

## Agent API (read-only by default)

```
GET  /agent/v1/state                host snapshot, uptime, version
GET  /agent/v1/monitoring/overview   one-shot CPU/RAM/disk
GET  /agent/v1/logs/recent?service=&range=
GET  /agent/v1/audit/recent?actor=&range=
GET  /agent/v1/packages/managed      inventory of panel-managed components
```

These endpoints are constant-time and bounded; no secret material is
ever returned, including TLS private keys, DB passwords, or
notification credentials.

## Recipe execution

The control plane never runs code on the agent directly. Instead it
ships a **signed recipe** — a JSON document with an Ed25519 signature
under a panel-controlled key. The agent verifies the signature, the
expiry, and the recipe's allowed operations, then executes the recipe
through the existing `software_center` package adapter (no new
execution path).

```
POST /agent/v1/recipes
  body: { manifest_b64, signature_b64, expiry, recipe_id }

  → 202 { execution_id }
GET  /agent/v1/recipes/{execution_id}
  → 200 { status, phase, progress_pct, redacted_diagnostics }
POST /agent/v1/recipes/{execution_id}/cancel
  → 200 or 409 if past safe-checkpoint
```

## Control-plane API

```
GET  /api/v1/fleet/agents
GET  /api/v1/fleet/agents/{id}/overview       proxy to agent
GET  /api/v1/fleet/agents/{id}/monitoring/overview
GET  /api/v1/fleet/agents/{id}/logs/recent
POST /api/v1/fleet/agents                     register (returns CSR challenge)
DELETE /api/v1/fleet/agents/{id}              deregister
```

The control plane never proxies raw byte streams; it returns
structured JSON with strict schemas.

## Resource budget

- Agent binary: same single-static-binary as the panel, with a
  `--mode agent` flag and a stripped surface.
- Idle RSS budget: ≤ 30 MiB (excluding sysinfo's own buffers).
- Disk budget for the agent DB: ≤ 50 MiB with retention pruning.
- No PHP, no plug-in runtime, no eval — same property as the panel.

## Tests

```
1.1  Unit: signature verification, host-fingerprint determinism,
     recipe manifest validation, allowlist enforcement.
1.2  Property: any tampered recipe signature is rejected;
     revoked certs are refused; CIDR-allowlist token never
     reaches a write route.
1.3  Service tests with mock adapters, mock clock, mock agent
     client covering registration, heartbeat, recipe dispatch,
     and progress streaming.
1.4  Integration: control plane + in-process agent; recipe end-to-
     end; revoke cert; agent refuses to continue.
1.5  CLI E2E: `openpanel fleet {list,register,deregister}` and
     agent-side `openpanel-agent register`.
1.6  Web: /fleet page with agent list, drill-down, and a
     deploy-recipe button (CSRF enforced).
```
