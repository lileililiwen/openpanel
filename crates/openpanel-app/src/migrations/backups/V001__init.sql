CREATE TABLE IF NOT EXISTS backup_plans (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_backup_plans_owner ON backup_plans(owner_id);

CREATE TABLE IF NOT EXISTS backup_runs (
    id TEXT PRIMARY KEY,
    plan_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_backup_runs_owner_created ON backup_runs(owner_id, created_at DESC);

CREATE TABLE IF NOT EXISTS restore_jobs (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL
);
