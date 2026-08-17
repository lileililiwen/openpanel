-- Web application installer v001: install plans, install runs,
-- installed records, and idempotency keys.

CREATE TABLE IF NOT EXISTS web_app_installs (
    install_id TEXT PRIMARY KEY NOT NULL,
    site_id TEXT NOT NULL,
    app_id TEXT NOT NULL,
    version TEXT NOT NULL,
    install_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    removed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_web_app_installs_site_app
    ON web_app_installs (site_id, app_id);

CREATE TABLE IF NOT EXISTS web_app_runs (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    app_id TEXT NOT NULL,
    install_id TEXT NOT NULL,
    install_path TEXT NOT NULL,
    post_install_url TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    failure_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_web_app_runs_plan
    ON web_app_runs (plan_id);

CREATE TABLE IF NOT EXISTS web_app_idempotency (
    key TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_app_idempotency_run
    ON web_app_idempotency (run_id);

CREATE TABLE IF NOT EXISTS web_app_plans (
    id TEXT PRIMARY KEY NOT NULL,
    app_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    artifacts_json TEXT NOT NULL,
    install_path TEXT NOT NULL,
    db_json TEXT NOT NULL,
    overlays_json TEXT NOT NULL,
    warnings_json TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    secret_ciphertext TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
