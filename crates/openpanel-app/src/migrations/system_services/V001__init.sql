CREATE TABLE IF NOT EXISTS service_health_history(id INTEGER PRIMARY KEY AUTOINCREMENT,service_id TEXT NOT NULL,health TEXT NOT NULL,observed_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_service_health_recent ON service_health_history(service_id,observed_at DESC);
