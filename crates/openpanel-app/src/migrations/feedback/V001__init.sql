CREATE TABLE IF NOT EXISTS feedback (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    sentiment TEXT NOT NULL CHECK (sentiment IN ('up','down')),
    comment TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_feedback_account_created
    ON feedback(account_id, created_at);
