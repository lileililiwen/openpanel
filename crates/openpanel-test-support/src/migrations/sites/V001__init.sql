-- Sites v0.1 initial schema.

CREATE TABLE IF NOT EXISTS sites (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    primary_domain TEXT NOT NULL UNIQUE,
    aliases TEXT NOT NULL DEFAULT '[]',
    document_root TEXT NOT NULL,
    php_enabled INTEGER NOT NULL DEFAULT 0,
    php_version TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sites_owner ON sites(owner_id);
CREATE INDEX IF NOT EXISTS idx_sites_status ON sites(status);
CREATE INDEX IF NOT EXISTS idx_sites_primary_domain ON sites(primary_domain);
