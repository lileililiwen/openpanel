-- Webmail client v001: short-lived session tokens.

CREATE TABLE IF NOT EXISTS webmail_session_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    mailbox TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    password_cipher TEXT NOT NULL,
    rotated_original_cipher TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webmail_session_mailbox
    ON webmail_session_tokens (mailbox);
