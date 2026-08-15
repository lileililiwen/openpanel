-- Reseller billing integration: usage meters, chargebacks,
-- and external integrations.

CREATE TABLE IF NOT EXISTS usage_meters (
    id            TEXT PRIMARY KEY NOT NULL,
    owner_id      TEXT NOT NULL,
    unit          TEXT NOT NULL,
    quantity      INTEGER NOT NULL,
    period_start  TEXT NOT NULL,
    period_end    TEXT NOT NULL,
    closed        INTEGER NOT NULL DEFAULT 0,
    recorded_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_meters_owner_period
    ON usage_meters (owner_id, period_start);

CREATE TABLE IF NOT EXISTS chargebacks (
    id              TEXT PRIMARY KEY NOT NULL,
    owner_id        TEXT NOT NULL,
    period_start    TEXT NOT NULL,
    period_end      TEXT NOT NULL,
    amount_minor    INTEGER NOT NULL,
    currency        TEXT NOT NULL,
    lines_json      TEXT NOT NULL DEFAULT '[]',
    finalised       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_chargebacks_owner
    ON chargebacks (owner_id, period_start);

CREATE TABLE IF NOT EXISTS billing_integrations (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    webhook_url     TEXT NOT NULL,
    webhook_secret  TEXT NOT NULL,
    status          TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
