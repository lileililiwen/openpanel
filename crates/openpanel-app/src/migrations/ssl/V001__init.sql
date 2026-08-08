-- SSL v0.1 initial schema.
-- Owned by the ssl bounded context. Other modules MUST NOT alter these tables.

CREATE TABLE IF NOT EXISTS certificates (
    id TEXT PRIMARY KEY,
    domain TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL,
    issuer TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT NOT NULL,
    key_type TEXT NOT NULL,
    cert_pem TEXT NOT NULL,
    chain_pem TEXT NOT NULL DEFAULT '',
    key_ciphertext BLOB NOT NULL,
    force_https INTEGER NOT NULL DEFAULT 1,
    acme_endpoint TEXT,
    created_at TEXT NOT NULL,
    renewed_at TEXT,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_certificates_domain ON certificates(domain);
CREATE INDEX IF NOT EXISTS idx_certificates_valid_to ON certificates(valid_to);
CREATE INDEX IF NOT EXISTS idx_certificates_source ON certificates(source);