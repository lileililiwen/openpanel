-- Compliance bounded context: hardening runs, retention policy
-- (singleton), and GDPR exports with secret redaction.

CREATE TABLE IF NOT EXISTS hardening_runs (
    id            TEXT PRIMARY KEY NOT NULL,
    profile       TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    completed_at  TEXT,
    initiated_by  TEXT NOT NULL,
    rules_json    TEXT NOT NULL DEFAULT '[]',
    has_failures  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_hardening_runs_started
    ON hardening_runs (started_at DESC);

CREATE TABLE IF NOT EXISTS audit_retention_policy (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    ttl_days             INTEGER NOT NULL,
    export_before_purge  INTEGER NOT NULL DEFAULT 0,
    updated_at           TEXT NOT NULL,
    updated_by           TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS gdpr_exports (
    id            TEXT PRIMARY KEY NOT NULL,
    user_id       TEXT NOT NULL,
    payload_json  TEXT NOT NULL,
    generated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_gdpr_exports_user
    ON gdpr_exports (user_id, generated_at DESC);
