-- site-staging v0.1 initial schema.
-- Owned by the site-staging bounded context. Other modules MUST NOT alter these tables.

CREATE TABLE IF NOT EXISTS staging_slots (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL UNIQUE,
    subdomain TEXT NOT NULL,
    document_root TEXT NOT NULL,
    db_name TEXT NOT NULL,
    php_version TEXT,
    sync_policy TEXT NOT NULL,
    schedule TEXT,
    current_snapshot INTEGER,
    last_promoted_snapshot INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_staging_slots_site
    ON staging_slots(site_id);

CREATE TABLE IF NOT EXISTS staging_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id TEXT NOT NULL,
    snapshot INTEGER NOT NULL,
    taken_at TEXT NOT NULL,
    UNIQUE (site_id, snapshot),
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_staging_snapshots_site
    ON staging_snapshots(site_id, snapshot DESC);

CREATE TABLE IF NOT EXISTS promotion_runs (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL,
    snapshot INTEGER NOT NULL,
    confirmed_at TEXT NOT NULL,
    status TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    requested_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    failure_reason TEXT,
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_promotion_runs_site
    ON promotion_runs(site_id, requested_at DESC);
CREATE INDEX IF NOT EXISTS idx_promotion_runs_status
    ON promotion_runs(status);

CREATE TABLE IF NOT EXISTS staging_locks (
    site_id TEXT PRIMARY KEY,
    locked_by TEXT NOT NULL,
    locked_at TEXT NOT NULL,
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);
