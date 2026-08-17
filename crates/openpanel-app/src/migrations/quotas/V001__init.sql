-- Quotas v0.1 initial schema.
-- Owned by the quotas bounded context. The `quota_policies` table
-- stores one row per subject (user or site); the `quota_usages`
-- table is append-only (one row per sample).

CREATE TABLE IF NOT EXISTS quota_policies (
    id TEXT PRIMARY KEY,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    disk_soft_bytes INTEGER NOT NULL,
    disk_hard_bytes INTEGER NOT NULL,
    disk_grace_days INTEGER NOT NULL,
    bandwidth_soft_bytes INTEGER NOT NULL,
    bandwidth_hard_bytes INTEGER NOT NULL,
    bandwidth_grace_days INTEGER NOT NULL,
    inodes_soft INTEGER NOT NULL,
    inodes_hard INTEGER NOT NULL,
    inodes_grace_days INTEGER NOT NULL,
    max_file_size_bytes INTEGER,
    cpu_shares INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (subject_kind, subject_id)
);

CREATE INDEX IF NOT EXISTS idx_quota_policies_subject
    ON quota_policies(subject_kind, subject_id);

CREATE TABLE IF NOT EXISTS quota_usages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subject_id TEXT NOT NULL,
    sampled_at TEXT NOT NULL,
    disk_used_bytes INTEGER NOT NULL,
    disk_inodes_used INTEGER NOT NULL,
    bandwidth_used_bytes INTEGER NOT NULL,
    over_soft_json TEXT NOT NULL,
    over_hard_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_quota_usages_subject_time
    ON quota_usages(subject_id, sampled_at DESC);
