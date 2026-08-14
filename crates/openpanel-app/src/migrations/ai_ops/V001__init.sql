-- AI Ops bounded context: sessions, messages, and proposed/executed
-- actions. Every row is owned by a user (sessions) or a session
-- (messages, actions). Actions in `Proposed` state are awaiting
-- approval; `Executed` actions carry the approver and audit id.

CREATE TABLE IF NOT EXISTS ai_sessions (
    id           TEXT PRIMARY KEY NOT NULL,
    owner_id     TEXT NOT NULL,
    title        TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ai_sessions_owner_updated
    ON ai_sessions (owner_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS ai_messages (
    id          TEXT PRIMARY KEY NOT NULL,
    session_id  TEXT NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    tool_results_json TEXT NOT NULL DEFAULT '[]',
    created_at  TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES ai_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_ai_messages_session_created
    ON ai_messages (session_id, created_at);

CREATE TABLE IF NOT EXISTS ai_actions (
    id           TEXT PRIMARY KEY NOT NULL,
    session_id   TEXT NOT NULL,
    tool_name    TEXT NOT NULL,
    kind         TEXT NOT NULL,
    params_json  TEXT NOT NULL,
    status       TEXT NOT NULL,
    approved_by  TEXT,
    audit_id     TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES ai_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_ai_actions_session
    ON ai_actions (session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_ai_actions_status_updated
    ON ai_actions (status, updated_at);
