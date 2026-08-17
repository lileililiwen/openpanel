# agent Specification

## Purpose

Introduces the agent bounded context: a per-host agent runtime
that talks to the control plane over mTLS, with `FleetToken`
fallback for environments where mTLS is impractical, and
signed `RecipeManifest`s the agent validates before executing.

## Requirements

### Requirement: Agent Registration

The agent context SHALL model an `AgentRegistration` aggregate carrying: `AgentId` (UUID), `host_fingerprint` (stable host identifier), `hostname`, `status` (`Pending | Online | Offline | Revoked`), `cert_fingerprint` (SHA-256 of the mTLS client cert), `last_heartbeat_at`, `registered_at`, and `owner_id`. The agent moves through `Pending -> Online -> Offline` on heartbeat activity and `* -> Revoked` on explicit revocation.

#### Scenario: Heartbeat moves the agent online

- **WHEN** an agent calls `POST /api/v1/agent/v1/heartbeat` with a valid mTLS cert
- **THEN** the agent's status flips to `Online` and `last_heartbeat_at = now`.

#### Scenario: Revoked agent cannot heartbeat

- **WHEN** an agent's status is `Revoked`
- **THEN** any subsequent API call is rejected with `AgentError::Revoked`.

### Requirement: FleetToken

The agent context SHALL issue scoped `FleetToken`s as a fallback to mTLS. The token hash (SHA-256 of the plaintext) is persisted; the plaintext is never stored. Tokens carry `FleetTokenScope` (`Read | Execute | Admin`) and a `revoked` flag.

#### Scenario: Expired token is rejected

- **WHEN** a token's `expires_at` is in the past
- **THEN** `resolve_token` returns `None`.

#### Scenario: Revoked token is rejected

- **WHEN** a token's `revoked` flag is `true`
- **THEN** `resolve_token` returns `None`.

### Requirement: Signed Recipe Manifest

A `RecipeManifest` carries a list of forward actions, a list of rollback actions, an `allowed_runners` set, a `signature`, and `signed_at` / `expires_at`. The agent MUST reject any manifest whose signature does not validate OR whose `expires_at` has passed.

#### Scenario: Manifest allows runner when present

- **WHEN** the manifest's `allowed_runners` contains `nginx`
- **THEN** dispatching with `runner = "nginx"` succeeds.

#### Scenario: Manifest rejects unknown runner

- **WHEN** the manifest's `allowed_runners` does NOT contain `mysql`
- **THEN** dispatching with `runner = "mysql"` is rejected with `AgentError::RunnerNotAllowed`.

#### Scenario: Expired manifest is rejected

- **WHEN** the manifest's `expires_at` is in the past
- **THEN** `validate_dispatch` returns `AgentError::ManifestExpired`.

### Requirement: Audit and Event Surface

Every agent lifecycle mutation SHALL emit an audit event with the actor, the agent id, and one of `{AgentRegistered, AgentRevoked, FleetTokenIssued, FleetTokenRevoked, RecipeManifestStored}`.

#### Scenario: Token issue is audited

- **WHEN** an Owner issues a `FleetToken` to an agent
- **THEN** the audit log records `FleetTokenIssued{actor, agent_id, scope}`.
