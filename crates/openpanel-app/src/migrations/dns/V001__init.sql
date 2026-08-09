CREATE TABLE IF NOT EXISTS dns_provider_accounts (
    id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
    capabilities TEXT NOT NULL, enabled INTEGER NOT NULL,
    encrypted_credential TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS dns_zones (
    id TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES dns_provider_accounts(id) ON DELETE CASCADE,
    remote_id TEXT NOT NULL, name TEXT NOT NULL, remote_version TEXT NOT NULL,
    last_success TEXT, last_error TEXT, drift_status TEXT NOT NULL,
    UNIQUE(account_id, remote_id)
);
CREATE TABLE IF NOT EXISTS dns_records (
    zone_id TEXT NOT NULL REFERENCES dns_zones(id) ON DELETE CASCADE,
    remote_id TEXT NOT NULL, name TEXT NOT NULL, data TEXT NOT NULL,
    ttl INTEGER NOT NULL, remote_version TEXT NOT NULL,
    PRIMARY KEY(zone_id, remote_id)
);
