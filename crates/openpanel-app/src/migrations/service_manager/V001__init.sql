-- Service manager bounded context: lifecycle action history.
-- ServiceInfo itself is read from systemd at request time; we
-- only persist the audit-style history rows.

CREATE TABLE IF NOT EXISTS service_action_history (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    action        TEXT NOT NULL,
    actor         TEXT NOT NULL,
    recorded_at   TEXT NOT NULL,
    success       INTEGER NOT NULL,
    message       TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_service_action_history_name
    ON service_action_history (name, recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_service_action_history_recorded
    ON service_action_history (recorded_at DESC);
