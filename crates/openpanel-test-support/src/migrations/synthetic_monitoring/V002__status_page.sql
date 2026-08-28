-- Status page: single-row policy aggregate keyed by `id = 1`,
-- plus per-check publish toggle and label.

CREATE TABLE IF NOT EXISTS status_page (
    id              INTEGER PRIMARY KEY NOT NULL DEFAULT 1 CHECK (id = 1),
    slug            TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS status_page_entries (
    check_id        TEXT PRIMARY KEY NOT NULL,
    label           TEXT NOT NULL
);