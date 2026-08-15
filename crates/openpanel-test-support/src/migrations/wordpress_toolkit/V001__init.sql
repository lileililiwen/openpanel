-- WordPress toolkit: managed sites and update runs.

CREATE TABLE IF NOT EXISTS wp_sites (
    id                     TEXT PRIMARY KEY NOT NULL,
    site_id                TEXT NOT NULL UNIQUE,
    wp_root                TEXT NOT NULL,
    core_version           TEXT NOT NULL,
    last_snapshot_db       TEXT,
    last_snapshot_files   TEXT,
    cache_mode             TEXT NOT NULL,
    registered_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS wp_update_runs (
    id              TEXT PRIMARY KEY NOT NULL,
    site_id         TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    updates_json    TEXT NOT NULL DEFAULT '[]',
    success         INTEGER NOT NULL,
    rolled_back     INTEGER NOT NULL DEFAULT 0,
    message         TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (site_id) REFERENCES wp_sites(site_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_wp_update_runs_site
    ON wp_update_runs (site_id, started_at DESC);
