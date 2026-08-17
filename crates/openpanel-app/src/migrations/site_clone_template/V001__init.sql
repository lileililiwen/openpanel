-- Site clone + template export v001: clone plans, clone runs,
-- templates, and anonymisation tokens.

CREATE TABLE IF NOT EXISTS site_templates (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    source_site_id TEXT NOT NULL,
    artifact_path TEXT NOT NULL,
    signature TEXT NOT NULL,
    signature_valid INTEGER NOT NULL DEFAULT 1,
    warnings TEXT NOT NULL,
    pii_policy TEXT NOT NULL DEFAULT 'none',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS clone_plans (
    id TEXT PRIMARY KEY NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    snapshot_id INTEGER,
    target_owner_id TEXT NOT NULL,
    target_domain TEXT NOT NULL,
    pii_policy TEXT NOT NULL,
    files_json TEXT NOT NULL,
    db_action_json TEXT NOT NULL,
    warnings_json TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS clone_runs (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL,
    target_site_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    snapshot_id INTEGER,
    pii_policy TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    failure_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_clone_runs_target
    ON clone_runs (target_site_id, started_at DESC);

CREATE TABLE IF NOT EXISTS anonymisation_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES clone_runs (id) ON DELETE CASCADE,
    cipher_text TEXT NOT NULL,
    original_hash TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_anonymisation_tokens_run
    ON anonymisation_tokens (run_id);
