-- Monitoring v0.1 initial schema.
-- Owned by the monitoring bounded context. Other modules MUST NOT alter
-- these tables.

-- One row per (timestamp, kind): an append-only time series. The
-- composite primary key enforces idempotence, and the index makes
-- history(kind, range) an index-only range scan.
CREATE TABLE IF NOT EXISTS monitoring_samples (
    ts    TEXT NOT NULL,  -- RFC 3339 UTC, second precision
    kind  TEXT NOT NULL,  -- 'Cpu' | 'Memory' | 'Disk' | 'Network'
    value REAL NOT NULL,
    PRIMARY KEY (ts, kind)
);

CREATE INDEX IF NOT EXISTS idx_monitoring_kind_ts
    ON monitoring_samples (kind, ts);
