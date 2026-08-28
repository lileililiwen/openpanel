-- Preview deployments: per-PR throwaway environments.

CREATE TABLE IF NOT EXISTS previews (
    id              TEXT PRIMARY KEY NOT NULL,
    site_id         TEXT NOT NULL,
    repo_id         TEXT NOT NULL,
    pr_number       INTEGER NOT NULL,
    state           TEXT NOT NULL,
    hostname        TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    ready_at        TEXT,
    expires_at      TEXT,
    destroyed_at    TEXT,
    destroy_reason  TEXT,
    FOREIGN KEY (repo_id) REFERENCES deploy_repos(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_previews_repo
    ON previews (repo_id, pr_number);
CREATE INDEX IF NOT EXISTS idx_previews_site
    ON previews (site_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_previews_expiry
    ON previews (expires_at);
