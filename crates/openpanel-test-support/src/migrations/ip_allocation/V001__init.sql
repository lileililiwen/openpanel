-- IPv6 + address-pool bounded context: pools and per-site
-- allocations.

CREATE TABLE IF NOT EXISTS ip_pools (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    family      TEXT NOT NULL,
    cidr        TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ip_allocations (
    id              TEXT PRIMARY KEY NOT NULL,
    pool_id         TEXT NOT NULL,
    address         TEXT NOT NULL,
    site_id         TEXT NOT NULL,
    status          TEXT NOT NULL,
    allocated_at    TEXT NOT NULL,
    bound_at        TEXT,
    FOREIGN KEY (pool_id) REFERENCES ip_pools(id) ON DELETE CASCADE,
    UNIQUE (pool_id, address)
);
CREATE INDEX IF NOT EXISTS idx_ip_allocations_site
    ON ip_allocations (site_id);
CREATE INDEX IF NOT EXISTS idx_ip_allocations_pool
    ON ip_allocations (pool_id);
