CREATE TABLE IF NOT EXISTS log_source_offsets (
    source_id TEXT NOT NULL,
    file_identity TEXT NOT NULL,
    byte_offset INTEGER NOT NULL,
    parse_errors INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source_id, file_identity, byte_offset)
);

CREATE TABLE IF NOT EXISTS traffic_hourly (
    site_id TEXT NOT NULL,
    hour TEXT NOT NULL,
    requests INTEGER NOT NULL,
    response_bytes INTEGER NOT NULL,
    status_2xx INTEGER NOT NULL,
    status_3xx INTEGER NOT NULL,
    status_4xx INTEGER NOT NULL,
    status_5xx INTEGER NOT NULL,
    latency_ms_total INTEGER NOT NULL,
    PRIMARY KEY (site_id, hour)
);

CREATE INDEX IF NOT EXISTS idx_traffic_hourly_hour ON traffic_hourly(hour);
