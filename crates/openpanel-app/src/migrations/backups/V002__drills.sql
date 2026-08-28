CREATE TABLE IF NOT EXISTS backup_drills (
    id TEXT PRIMARY KEY,
    backup_run_id TEXT NOT NULL,
    state TEXT NOT NULL,
    assertions_json TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_backup_drills_run_stated ON backup_drills(backup_run_id, started_at DESC);
