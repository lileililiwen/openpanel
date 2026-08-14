-- Per-site collaborators initial schema (V001).
-- Records collaborator accounts and their per-site grants. The
-- permission set is a small bit-flag (0..=0x0F).

CREATE TABLE IF NOT EXISTS collaborators (
    collaborator_id TEXT NOT NULL PRIMARY KEY,
    account_id      TEXT NOT NULL,
    email           TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN
                      ('invited', 'active', 'revoked')),
    invited_at      TEXT NOT NULL,
    accepted_at     TEXT,
    revoked_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_collaborators_account
    ON collaborators(account_id);

CREATE TABLE IF NOT EXISTS site_grants (
    collaborator_id TEXT NOT NULL,
    site_id         TEXT NOT NULL,
    permissions     INTEGER NOT NULL CHECK (permissions >= 0 AND permissions <= 15),
    granted_at      TEXT NOT NULL,
    PRIMARY KEY (collaborator_id, site_id)
);

CREATE INDEX IF NOT EXISTS idx_site_grants_site
    ON site_grants(site_id);