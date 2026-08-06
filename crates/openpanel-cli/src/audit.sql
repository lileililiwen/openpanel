-- Mirrors the SQL shipped in openpanel-app/migrations/000_audit.sql. The CLI
-- uses this embedded copy to ensure the audit table exists before any
-- module records an event.
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT,
    source_ip TEXT,
    outcome TEXT NOT NULL,
    metadata TEXT
);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log(actor);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts);