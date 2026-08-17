-- Migration importers v0.1 initial schema.
-- Owned by the migration-importers bounded context. The
-- `migration_runs` table records one committed (or rolled-back)
-- import, `imported_resources` stores the per-resource outcomes
-- used by rollback, and `translation_log_entries` keeps the
-- redacted diagnostics for the run.

CREATE TABLE IF NOT EXISTS migration_runs (
    id TEXT PRIMARY KEY,
    plan_id TEXT NOT NULL,
    driver TEXT NOT NULL,
    target_owner_user_id TEXT NOT NULL,
    confirmed_at TEXT NOT NULL,
    status TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_migration_runs_status ON migration_runs(status);
CREATE INDEX IF NOT EXISTS idx_migration_runs_plan ON migration_runs(plan_id);

CREATE TABLE IF NOT EXISTS imported_resources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_key TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    rolled_back INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (run_id) REFERENCES migration_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_imported_resources_run ON imported_resources(run_id);

CREATE TABLE IF NOT EXISTS translation_log_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_key TEXT NOT NULL,
    outcome TEXT NOT NULL,
    redacted TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES migration_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_translation_log_run ON translation_log_entries(run_id);