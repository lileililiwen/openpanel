CREATE TABLE IF NOT EXISTS docker_containers (
    id TEXT PRIMARY KEY NOT NULL,
    spec_json TEXT NOT NULL,
    env_cipher TEXT NOT NULL,
    runtime_id TEXT,
    status TEXT NOT NULL,
    oom_killed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS docker_image_allowlist (
    pattern TEXT PRIMARY KEY NOT NULL,
    allow_pull INTEGER NOT NULL,
    pin_digest_required INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS docker_stacks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    stack_cipher TEXT NOT NULL,
    status TEXT NOT NULL,
    last_applied_at TEXT
);
