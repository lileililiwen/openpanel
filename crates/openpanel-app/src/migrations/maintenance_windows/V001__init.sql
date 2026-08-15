-- Scheduled maintenance windows: per-window records + per-
-- override single-use tokens.

CREATE TABLE IF NOT EXISTS maintenance_windows (
    id                  TEXT PRIMARY KEY NOT NULL,
    label               TEXT NOT NULL,
    starts_at           TEXT NOT NULL,
    ends_at             TEXT NOT NULL,
    blocked_classes     TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    created_by          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maintenance_windows_starts
    ON maintenance_windows (starts_at);

CREATE TABLE IF NOT EXISTS maintenance_overrides (
    id                  TEXT PRIMARY KEY NOT NULL,
    target_class        TEXT NOT NULL,
    reason              TEXT NOT NULL,
    ttl_secs            INTEGER NOT NULL,
    created_at          TEXT NOT NULL,
    expires_at          TEXT NOT NULL,
    consumed_at         TEXT,
    issued_by           TEXT NOT NULL
);