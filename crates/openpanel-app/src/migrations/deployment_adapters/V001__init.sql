-- Deployment adapters: idempotency records and evidence.
--
-- The bounded context is projection-first; the in-memory
-- `DeploymentAdapterService` owns the canonical evidence map
-- and the schema is here so a follow-on change can swap the
-- service for a SQLite-backed repository without a re-merge.
--
-- The two tables are intentionally simple; columns are bounded
-- by the domain validators in
-- `openpanel_domain::deployment_adapters` (release digest,
-- target, adapter id, actor all <= 254 chars; diagnostic <=
-- 1024 chars).

CREATE TABLE IF NOT EXISTS deployment_operation_keys (
    id              TEXT PRIMARY KEY NOT NULL,
    target          TEXT NOT NULL,
    adapter         TEXT NOT NULL,
    action          TEXT NOT NULL,
    operation_value TEXT NOT NULL,
    release_digest  TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE (target, adapter, action, operation_value, release_digest)
);

CREATE TABLE IF NOT EXISTS deployment_evidence (
    id              TEXT PRIMARY KEY NOT NULL,
    target          TEXT NOT NULL,
    adapter         TEXT NOT NULL,
    action          TEXT NOT NULL,
    release_digest  TEXT NOT NULL,
    state           TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT NOT NULL,
    diagnostic      TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_deployment_evidence_target
    ON deployment_evidence (target, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_deployment_evidence_release
    ON deployment_evidence (release_digest, started_at DESC);
