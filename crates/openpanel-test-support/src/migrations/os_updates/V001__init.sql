-- OS update management: singleton policy and apply history.

CREATE TABLE IF NOT EXISTS update_policy (
    id                       INTEGER PRIMARY KEY CHECK (id = 1),
    security_auto_install    INTEGER NOT NULL,
    other_auto_install       INTEGER NOT NULL,
    run_hour                 INTEGER NOT NULL,
    auto_reboot              INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS update_history (
    id              TEXT PRIMARY KEY NOT NULL,
    actor           TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    kind            TEXT NOT NULL,
    package_count   INTEGER NOT NULL,
    success         INTEGER NOT NULL,
    message         TEXT NOT NULL DEFAULT '',
    reboot_required INTEGER NOT NULL DEFAULT 0,
    kernel_updated  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_update_history_started
    ON update_history (started_at DESC);
