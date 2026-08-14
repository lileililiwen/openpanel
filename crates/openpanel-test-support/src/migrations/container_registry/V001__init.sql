-- Container registry initial schema (V001).
-- Per-user namespaces, stored images, scan results, and a global
-- configuration row (id=1). The blob data lives on disk under
-- RegistryConfig.storage_root and this schema records the metadata.

CREATE TABLE IF NOT EXISTS registry_config (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    storage_root    TEXT NOT NULL,
    retention_json  TEXT NOT NULL,
    scan_on_push    INTEGER NOT NULL CHECK (scan_on_push IN (0, 1))
);

CREATE TABLE IF NOT EXISTS image_namespaces (
    namespace_id    TEXT NOT NULL PRIMARY KEY,
    owner           TEXT NOT NULL,
    quota_bytes     INTEGER NOT NULL CHECK (quota_bytes >= 0),
    used_bytes      INTEGER NOT NULL CHECK (used_bytes >= 0),
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_image_namespaces_owner
    ON image_namespaces(owner);

CREATE TABLE IF NOT EXISTS stored_images (
    digest          TEXT NOT NULL,
    namespace       TEXT NOT NULL,
    size_bytes      INTEGER NOT NULL CHECK (size_bytes >= 0),
    pushed_at       TEXT NOT NULL,
    reference       TEXT,
    scan_status     TEXT NOT NULL CHECK (scan_status IN
                      ('pending', 'clean', 'with_findings', 'failed')),
    PRIMARY KEY (namespace, digest)
);

CREATE INDEX IF NOT EXISTS idx_stored_images_namespace_pushed_at
    ON stored_images(namespace, pushed_at);

CREATE TABLE IF NOT EXISTS scan_results (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    digest          TEXT NOT NULL,
    scanned_at      TEXT NOT NULL,
    findings_json   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scan_results_digest_scanned_at
    ON scan_results(digest, scanned_at);