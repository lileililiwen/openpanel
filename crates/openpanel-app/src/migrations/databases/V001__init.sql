-- Databases v0.1 initial schema.
-- Owned by the databases bounded context. Other modules MUST NOT alter these tables.

CREATE TABLE IF NOT EXISTS databases (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    db_user TEXT NOT NULL,
    db_host TEXT NOT NULL,
    engine TEXT NOT NULL,
    charset TEXT NOT NULL,
    status TEXT NOT NULL,
    password_ciphertext TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_databases_owner ON databases(owner_id);
CREATE INDEX IF NOT EXISTS idx_databases_status ON databases(status);
CREATE INDEX IF NOT EXISTS idx_databases_name ON databases(name);