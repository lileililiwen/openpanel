-- Account hierarchy v0.1 initial schema.
-- Owned by the account-hierarchy bounded context. The relationship
-- table is append-only. The parent_account_id column on users
-- (added by the identity v005 migration) is the canonical pointer
-- for active parents. The detached rows here are retained for
-- audit and re-parenting history.

CREATE TABLE IF NOT EXISTS account_relationships (
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    status TEXT NOT NULL,
    PRIMARY KEY (parent_id, child_id),
    FOREIGN KEY (parent_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (child_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_account_relationships_parent
    ON account_relationships(parent_id, status);
CREATE INDEX IF NOT EXISTS idx_account_relationships_child
    ON account_relationships(child_id, status);

CREATE TABLE IF NOT EXISTS quota_pools (
    parent_id TEXT NOT NULL,
    axis TEXT NOT NULL,
    total_bytes INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (parent_id, axis),
    FOREIGN KEY (parent_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pool_claims (
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    axis TEXT NOT NULL,
    share_bytes INTEGER NOT NULL,
    claimed_at TEXT NOT NULL,
    PRIMARY KEY (parent_id, child_id, axis),
    FOREIGN KEY (parent_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (child_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pool_claims_axis
    ON pool_claims(parent_id, axis);
