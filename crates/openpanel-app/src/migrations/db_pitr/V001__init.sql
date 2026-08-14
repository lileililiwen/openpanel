-- db-pitr v0.1 initial schema.
-- Owned by the db-pitr bounded context. Other modules MUST NOT alter these tables.

CREATE TABLE IF NOT EXISTS binlog_streams (
    id TEXT PRIMARY KEY,
    database_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    last_flushed TEXT NOT NULL,
    continuous INTEGER NOT NULL CHECK (continuous IN (0, 1)),
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (database_id) REFERENCES databases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_binlog_streams_database
    ON binlog_streams(database_id);
CREATE INDEX IF NOT EXISTS idx_binlog_streams_status
    ON binlog_streams(status);

CREATE TABLE IF NOT EXISTS pitr_restores (
    id TEXT PRIMARY KEY,
    database_id TEXT NOT NULL,
    request_ts TEXT NOT NULL,
    base_backup TEXT,
    replay_to TEXT,
    staging_db_id TEXT,
    status TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    requested_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    failure_reason TEXT,
    FOREIGN KEY (database_id) REFERENCES databases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pitr_restores_database
    ON pitr_restores(database_id, requested_at DESC);
CREATE INDEX IF NOT EXISTS idx_pitr_restores_status
    ON pitr_restores(status);

CREATE TABLE IF NOT EXISTS incremental_backups (
    id TEXT PRIMARY KEY,
    database_id TEXT NOT NULL,
    base_backup TEXT NOT NULL,
    delta_ref TEXT NOT NULL,
    mode TEXT NOT NULL,
    bytes INTEGER NOT NULL CHECK (bytes >= 0),
    captured_through TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    FOREIGN KEY (database_id) REFERENCES databases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_incremental_backups_database
    ON incremental_backups(database_id, captured_at DESC);
