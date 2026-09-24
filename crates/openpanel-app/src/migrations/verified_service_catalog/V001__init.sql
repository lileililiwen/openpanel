-- verified_service_catalog V001: signed catalog entries, lifecycle
-- jobs, the curated publisher registry, and the active revocation
-- list. The app service currently keeps this shape in memory; this
-- schema is the ready-to-swap persistence contract for the
-- follow-on store work (same convention as deployment_adapters /
-- application_delivery).

CREATE TABLE IF NOT EXISTS vsc_publishers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    trusted_by TEXT NOT NULL,
    trusted_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS vsc_entries (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    publisher_id TEXT NOT NULL REFERENCES vsc_publishers(id),
    digest TEXT NOT NULL,
    signature TEXT NOT NULL,
    trust_state TEXT NOT NULL CHECK (trust_state IN ('verified', 'community', 'revoked', 'blocked')),
    manifest_json TEXT NOT NULL,
    installed_digest TEXT,
    previous_digest TEXT,
    backup_taken INTEGER NOT NULL DEFAULT 0,
    submitted_by TEXT NOT NULL,
    submitted_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_vsc_entries_trust ON vsc_entries(trust_state);
CREATE INDEX IF NOT EXISTS idx_vsc_entries_publisher ON vsc_entries(publisher_id);

CREATE TABLE IF NOT EXISTS vsc_revocations (
    kind TEXT NOT NULL CHECK (kind IN ('publisher', 'digest')),
    value TEXT NOT NULL,
    revoked_by TEXT NOT NULL,
    revoked_at TEXT NOT NULL,
    PRIMARY KEY (kind, value)
);

CREATE TABLE IF NOT EXISTS vsc_jobs (
    id TEXT PRIMARY KEY,
    entry_id TEXT NOT NULL REFERENCES vsc_entries(id),
    phase TEXT NOT NULL CHECK (phase IN ('install', 'upgrade', 'backup', 'health', 'rollback', 'remove')),
    outcome TEXT NOT NULL CHECK (outcome IN ('pending', 'running', 'succeeded', 'failed', 'rolled_back')),
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL,
    last_error TEXT,
    guidance TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_vsc_jobs_entry ON vsc_jobs(entry_id);
CREATE INDEX IF NOT EXISTS idx_vsc_jobs_outcome ON vsc_jobs(outcome);
