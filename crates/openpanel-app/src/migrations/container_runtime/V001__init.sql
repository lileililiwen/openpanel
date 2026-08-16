-- Container runtime: per-user quota, registry credentials, metrics
-- samples, and monthly egress accounts. Builds on top of the docker
-- bounded context (which holds the desired-state / lifecycle rows).

-- Per-user container quota. One row per owner, inserted/updated by the
-- service's set_quota call and seeded (lazily) on the first
-- check_quota call when no row exists.
CREATE TABLE IF NOT EXISTS container_quotas (
    user_id                     TEXT PRIMARY KEY NOT NULL,
    max_concurrent              INTEGER NOT NULL,
    max_total                   INTEGER NOT NULL,
    cpu_pct_max                 INTEGER NOT NULL,
    memory_bytes_max            INTEGER NOT NULL,
    egress_bytes_per_month      INTEGER NOT NULL,
    updated_at                  TEXT NOT NULL
);

-- Append-only per-container metrics samples. The metrics poller
-- prunes rows older than the retention boundary (default 1 hour).
CREATE TABLE IF NOT EXISTS container_metrics_samples (
    container_id    TEXT NOT NULL,
    user_id         TEXT NOT NULL,
    cpu_pct         INTEGER NOT NULL,
    memory_bytes    INTEGER NOT NULL,
    net_rx          INTEGER NOT NULL,
    net_tx          INTEGER NOT NULL,
    exits           INTEGER NOT NULL,
    sampled_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_metrics_container_time
    ON container_metrics_samples (container_id, sampled_at);

-- Monthly egress accounts. `container_id` is the literal string
-- `""` for a per-user aggregate row, otherwise the container's
-- UUID string. Stored as `TEXT NOT NULL` so SQLite's conflict
-- detection is well-defined under a NULL composite primary key
-- (NULLs in SQLite indices are considered distinct on conflict).
CREATE TABLE IF NOT EXISTS network_egress_accounts (
    user_id        TEXT NOT NULL,
    container_id   TEXT NOT NULL,
    month          TEXT NOT NULL,
    bytes          INTEGER NOT NULL,
    PRIMARY KEY (user_id, container_id, month)
);

-- Registry credentials. The `encrypted_secret` column stores the
-- hex(nonce)||":"||hex(ciphertext) layout produced by
-- container_runtime::crypto (matches Agents.md §6).
CREATE TABLE IF NOT EXISTS registry_credentials (
    id               TEXT PRIMARY KEY NOT NULL,
    user_id          TEXT NOT NULL,
    registry         TEXT NOT NULL,
    username         TEXT NOT NULL,
    encrypted_secret TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    last_used_at     TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_registry_user_registry
    ON registry_credentials (user_id, registry);
CREATE INDEX IF NOT EXISTS idx_registry_user
    ON registry_credentials (user_id);
