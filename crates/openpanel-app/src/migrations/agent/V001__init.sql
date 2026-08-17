-- Agent v0.1 initial schema.
-- Owned by the agent bounded context. The `agents` table is the
-- control-plane view of every registered host; the `fleet_tokens`
-- table holds the bearer-token fallback for hosts that cannot
-- present an mTLS client cert; the `recipe_manifests` table stores
-- signed manifests the agent has been asked to execute.

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    host_fingerprint TEXT NOT NULL,
    hostname TEXT NOT NULL,
    status TEXT NOT NULL,
    cert_fingerprint TEXT NOT NULL,
    last_heartbeat_at TEXT,
    registered_at TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    UNIQUE (cert_fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_agents_owner ON agents(owner_id);
CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);

CREATE TABLE IF NOT EXISTS fleet_tokens (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_fleet_tokens_agent ON fleet_tokens(agent_id);

CREATE TABLE IF NOT EXISTS recipe_manifests (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    rollback_actions_json TEXT NOT NULL,
    allowed_runners_json TEXT NOT NULL,
    signature TEXT NOT NULL,
    signed_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recipe_manifests_name ON recipe_manifests(name);
