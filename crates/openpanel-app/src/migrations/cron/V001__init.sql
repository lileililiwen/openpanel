CREATE TABLE IF NOT EXISTS cron_jobs (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    payload TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cron_jobs_owner ON cron_jobs(owner_id);

CREATE TABLE IF NOT EXISTS cron_runs (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES cron_jobs(id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cron_runs_owner_created ON cron_runs(owner_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_cron_one_active_run ON cron_runs(job_id) WHERE active = 1;
