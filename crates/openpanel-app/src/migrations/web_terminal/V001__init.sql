CREATE TABLE IF NOT EXISTS web_terminal_tickets (
    token TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_web_terminal_tickets_expiry
    ON web_terminal_tickets (expires_at);

CREATE TABLE IF NOT EXISTS web_terminal_sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    state TEXT NOT NULL DEFAULT 'open',
    close_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_web_terminal_sessions_user_state
    ON web_terminal_sessions (user_id, state);
