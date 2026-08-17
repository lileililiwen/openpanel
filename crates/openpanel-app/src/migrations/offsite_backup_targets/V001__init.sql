-- Offsite backup targets v001: encrypted credentials, per-plan
-- remote targets, and master-key-wrapped KEK records.

CREATE TABLE IF NOT EXISTS backup_credentials (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('s3', 'wasabi', 'b2', 'rsync')),
    label TEXT NOT NULL,
    secret_enc TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS backup_remote_targets (
    plan_id TEXT PRIMARY KEY NOT NULL,
    credential_id TEXT NOT NULL REFERENCES backup_credentials (id) ON DELETE RESTRICT,
    prefix TEXT NOT NULL,
    schedule TEXT
);

CREATE INDEX IF NOT EXISTS idx_backup_remote_targets_credential
    ON backup_remote_targets (credential_id);

CREATE TABLE IF NOT EXISTS backup_kek_wrappers (
    id TEXT PRIMARY KEY NOT NULL,
    salt_hex TEXT NOT NULL,
    wrapped_kek_hex TEXT NOT NULL,
    created_at TEXT NOT NULL
);