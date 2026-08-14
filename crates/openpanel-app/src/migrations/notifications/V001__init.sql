CREATE TABLE IF NOT EXISTS notification_channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('smtp','webhook')),
    endpoint TEXT NOT NULL,
    port INTEGER,
    username TEXT,
    credential_enc TEXT NOT NULL,
    from_addr TEXT,
    tls_mode TEXT,
    allowlist_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    disabled_at TEXT
);

CREATE TABLE IF NOT EXISTS notification_subscriptions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    channel_id TEXT NOT NULL REFERENCES notification_channels(id),
    destination TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('alert','audit','job_terminal')),
    filter_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notification_subscriptions_match
    ON notification_subscriptions(kind, enabled);

CREATE TABLE IF NOT EXISTS notification_events (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    subject TEXT NOT NULL,
    severity TEXT NOT NULL,
    details_json TEXT NOT NULL,
    occurred_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notification_deliveries (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES notification_events(id),
    channel_id TEXT NOT NULL REFERENCES notification_channels(id),
    subscription_id TEXT NOT NULL REFERENCES notification_subscriptions(id),
    payload_digest TEXT NOT NULL,
    status TEXT NOT NULL,
    attempt_n INTEGER NOT NULL DEFAULT 0,
    next_retry_at TEXT,
    lease_until TEXT,
    last_error_redacted TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE(event_id, subscription_id)
);
CREATE INDEX IF NOT EXISTS idx_notification_deliveries_due
    ON notification_deliveries(status, next_retry_at, lease_until);
