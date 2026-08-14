-- Synthetic monitoring: checks and per-run results.

CREATE TABLE IF NOT EXISTS synthetic_checks (
    id                  TEXT PRIMARY KEY NOT NULL,
    name                TEXT NOT NULL,
    kind                TEXT NOT NULL,
    target              TEXT NOT NULL,
    expected_status     INTEGER,
    timeout_secs        INTEGER NOT NULL,
    throttle_secs       INTEGER NOT NULL,
    warn_before_days    INTEGER NOT NULL,
    created_at          TEXT NOT NULL,
    last_run_at         TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_synthetic_checks_enabled
    ON synthetic_checks (enabled);

CREATE TABLE IF NOT EXISTS synthetic_check_runs (
    id                  TEXT PRIMARY KEY NOT NULL,
    check_id            TEXT NOT NULL,
    ran_at              TEXT NOT NULL,
    latency_ms          INTEGER NOT NULL,
    http_status         INTEGER,
    cert_days_remaining INTEGER,
    status              TEXT NOT NULL,
    message             TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (check_id) REFERENCES synthetic_checks(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_synthetic_check_runs_check
    ON synthetic_check_runs (check_id, ran_at DESC);
