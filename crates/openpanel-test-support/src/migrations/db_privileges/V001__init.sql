-- Database privilege management: grants, remote access, SSO
-- sessions for the admin tool launcher.

CREATE TABLE IF NOT EXISTS db_grants (
    id            TEXT PRIMARY KEY NOT NULL,
    database_id   TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    scope_kind    TEXT NOT NULL,
    scope_name    TEXT,
    privilege     TEXT NOT NULL,
    granted_by    TEXT NOT NULL,
    granted_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_db_grants_database
    ON db_grants (database_id, user_id);

CREATE TABLE IF NOT EXISTS remote_access (
    database_id        TEXT PRIMARY KEY NOT NULL,
    enabled            INTEGER NOT NULL DEFAULT 0,
    allow_cidrs_json   TEXT NOT NULL DEFAULT '[]',
    wildcard_opt_in    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS admin_tool_sessions (
    id            TEXT PRIMARY KEY NOT NULL,
    database_id   TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    token         TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    expires_at    TEXT NOT NULL,
    consumed_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_admin_tool_sessions_expiry
    ON admin_tool_sessions (expires_at);
