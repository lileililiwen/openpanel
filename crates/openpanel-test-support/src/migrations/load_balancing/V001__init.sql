-- Load balancing and failover: pools and members.

CREATE TABLE IF NOT EXISTS lb_pools (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    algorithm   TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lb_members (
    id                    TEXT PRIMARY KEY NOT NULL,
    pool_id               TEXT NOT NULL,
    address               TEXT NOT NULL,
    weight                INTEGER NOT NULL,
    status                TEXT NOT NULL,
    last_probe_at         TEXT,
    failed_probe_count    INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (pool_id) REFERENCES lb_pools(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_lb_members_pool
    ON lb_members (pool_id);
