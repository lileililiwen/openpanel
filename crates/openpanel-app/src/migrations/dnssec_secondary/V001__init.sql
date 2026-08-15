-- DNSSEC + secondary DNS: policies, keys, secondaries, glue, DS.

CREATE TABLE IF NOT EXISTS dnssec_policies (
    zone_id       TEXT PRIMARY KEY NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 0,
    algorithm     TEXT NOT NULL,
    enabled_at    TEXT
);

CREATE TABLE IF NOT EXISTS zone_signing_keys (
    id                    TEXT PRIMARY KEY NOT NULL,
    zone_id               TEXT NOT NULL,
    role                  TEXT NOT NULL,
    algorithm             TEXT NOT NULL,
    key_tag               INTEGER NOT NULL,
    public_digest         TEXT NOT NULL,
    active                INTEGER NOT NULL,
    rollover_in_progress  INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_zone_signing_keys_zone
    ON zone_signing_keys (zone_id);

CREATE TABLE IF NOT EXISTS secondary_ns (
    id               TEXT PRIMARY KEY NOT NULL,
    zone_id          TEXT NOT NULL,
    address          TEXT NOT NULL,
    allowed_cidrs_json TEXT NOT NULL DEFAULT '[]',
    added_at         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_secondary_ns_zone
    ON secondary_ns (zone_id);

CREATE TABLE IF NOT EXISTS glue_records (
    id         TEXT PRIMARY KEY NOT NULL,
    zone_id    TEXT NOT NULL,
    name       TEXT NOT NULL,
    a          TEXT,
    aaaa       TEXT
);
CREATE INDEX IF NOT EXISTS idx_glue_records_zone
    ON glue_records (zone_id);

CREATE TABLE IF NOT EXISTS ds_records (
    zone_id     TEXT NOT NULL,
    key_tag     INTEGER NOT NULL,
    algorithm   INTEGER NOT NULL,
    digest_type INTEGER NOT NULL,
    digest      TEXT NOT NULL,
    PRIMARY KEY (zone_id, key_tag, digest_type)
);