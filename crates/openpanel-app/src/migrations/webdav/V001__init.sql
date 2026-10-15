CREATE TABLE IF NOT EXISTS webdav_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    site_id TEXT NOT NULL,
    username TEXT NOT NULL,
    home_abs TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    read_only INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_login_at TEXT,
    last_login_ip TEXT,
    created_at TEXT NOT NULL,
    disabled_at TEXT,
    UNIQUE(site_id, username),
    FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_webdav_accounts_site ON webdav_accounts(site_id, username);
