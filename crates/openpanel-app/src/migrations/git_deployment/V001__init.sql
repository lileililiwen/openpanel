-- Git deployment: per-site repos and deploy runs.

CREATE TABLE IF NOT EXISTS deploy_repos (
    id                  TEXT PRIMARY KEY NOT NULL,
    site_id             TEXT NOT NULL UNIQUE,
    url                 TEXT NOT NULL,
    branch              TEXT NOT NULL,
    linked_at           TEXT NOT NULL,
    build_command       TEXT NOT NULL DEFAULT '',
    docroot_subdir      TEXT NOT NULL DEFAULT '',
    webhook_secret      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS deploy_runs (
    id              TEXT PRIMARY KEY NOT NULL,
    repo_id         TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    commit_sha      TEXT NOT NULL,
    status          TEXT NOT NULL,
    message         TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (repo_id) REFERENCES deploy_repos(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_deploy_runs_repo
    ON deploy_runs (repo_id, started_at DESC);