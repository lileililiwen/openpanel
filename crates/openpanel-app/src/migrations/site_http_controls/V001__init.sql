CREATE TABLE IF NOT EXISTS site_http_controls (
    site_id TEXT PRIMARY KEY,
    document_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
