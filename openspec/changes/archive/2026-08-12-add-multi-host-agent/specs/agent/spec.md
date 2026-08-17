## ADDED Requirements

### Requirement: Agent Identity and Mutual TLS

Each agent SHALL identify itself with a stable host fingerprint and authenticate to the control plane with mTLS. The agent MUST refuse any non-mTLS connection and MUST reject any peer whose certificate is not signed by the pinned control-plane CA. The certificate's SAN SHALL be the host fingerprint; certificates past their `not_after` or on the revocation list SHALL be refused.

#### Scenario: Agent enrolls

- **WHEN** a freshly installed agent submits a registration request with a CSR
- **THEN** the control plane returns a signed client certificate, persists the agent record, and the agent begins heartbeating over mTLS.

#### Scenario: Revoked cert

- **WHEN** the control plane revokes an agent's certificate
- **THEN** the next connection from that agent is refused at the TLS handshake and a `AgentRevoked` audit event is written.

### Requirement: Agent Read-Only API

The agent SHALL expose a bounded set of read-only HTTP routes: `/agent/v1/state`, `/agent/v1/monitoring/overview`, `/agent/v1/logs/recent`, `/agent/v1/audit/recent`, `/agent/v1/packages/managed`. None of these routes SHALL return secret material (TLS private keys, DB passwords, notification credentials). All responses are constant-time and bounded.

#### Scenario: Secret leak attempt

- **WHEN** any read route is asked for a secret by any caller
- **THEN** the route returns a 404 and the request is audited as a `SecretRequestDenied`.

### Requirement: Signed Recipe Execution

A control plane SHALL ship a recipe to an agent only as a signed manifest (`manifest_b64`, `signature_b64`, `expiry`, `recipe_id`). The agent MUST verify the Ed25519 signature, the manifest schema, the expiry, and the recipe's allowlisted operations before executing. Execution goes through the existing software-center package adapter — no new execution path. Progress is streamed over the same mTLS connection.

#### Scenario: Tampered recipe

- **WHEN** the signature does not validate, the expiry has passed, or the schema is unknown
- **THEN** the agent returns 400 and writes `RecipeRejected`; no process is started.

#### Scenario: Recipe progress

- **WHEN** an authorized recipe runs
- **THEN** the agent streams `phase`, `progress_pct`, and redacted diagnostics over `GET /agent/v1/recipes/{execution_id}`.

#### Scenario: Cancel at safe checkpoint

- **WHEN** the control plane requests cancellation between declared safe phases
- **THEN** the agent records the cancellation and stops at the next safe checkpoint; cancellation during a non-interruptible phase is queued.

### Requirement: Fleet Aggregation

The control plane SHALL aggregate per-agent metadata: `agent_id`, `host_fingerprint`, `last_seen`, `version`, current `state`, and a per-agent health roll-up. Aggregation is read-only; the control plane never proxies raw byte streams from the agent.

#### Scenario: Heartbeat staleness

- **WHEN** an agent has not heartbeated within `OPENPANEL__FLEET__STALE_AFTER_SECS` (default 300)
- **THEN** the fleet view marks the agent `stale` and surfaces it in the web UI.

#### Scenario: Fleet list

- **WHEN** an Owner requests `GET /api/v1/fleet/agents`
- **THEN** the response contains all registered agents with their health roll-up and last-seen timestamp.

### Requirement: Fleet Surfaces

REST, CLI, and `/fleet` web surfaces SHALL support agent list, drill-down, recipe dispatch, and deregistration. Browser mutations MUST enforce CSRF. Recipes SHALL never be displayed with their raw manifest bytes in the UI; only the human-readable summary and the latest progress.

#### Scenario: Web recipe dispatch

- **WHEN** an Owner clicks Deploy on `/fleet/agents/{id}`
- **THEN** the panel posts the signed recipe over mTLS, polls progress, and renders phase transitions without exposing command arguments or secrets.
