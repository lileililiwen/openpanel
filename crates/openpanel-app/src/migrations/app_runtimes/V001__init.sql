-- Non-PHP runtime bounded context: per-site runtime config.

CREATE TABLE IF NOT EXISTS site_runtimes (
    id              TEXT PRIMARY KEY NOT NULL,
    site_id         TEXT NOT NULL UNIQUE,
    kind            TEXT NOT NULL,
    version         TEXT NOT NULL,
    app_port        INTEGER NOT NULL,
    workdir         TEXT NOT NULL,
    start_command   TEXT NOT NULL DEFAULT '',
    registered_at   TEXT NOT NULL,
    status          TEXT NOT NULL
);