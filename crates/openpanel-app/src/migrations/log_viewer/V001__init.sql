-- Log viewer bounded context: persisted download audit rows.
-- The viewer itself reads from the observability-export JSONL
-- store at request time; only the download history is durably
-- stored here.

CREATE TABLE IF NOT EXISTS log_downloads (
    id            TEXT PRIMARY KEY NOT NULL,
    actor         TEXT NOT NULL,
    source        TEXT NOT NULL,
    service       TEXT NOT NULL,
    line_count    INTEGER NOT NULL,
    downloaded_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_log_downloads_actor
    ON log_downloads (actor, downloaded_at DESC);
