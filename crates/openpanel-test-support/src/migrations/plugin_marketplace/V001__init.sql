-- Plugin marketplace initial schema (V001).
--
-- Caches verified catalogs by digest. The cache is keyed by digest
-- so a tampered catalog cannot overwrite a verified one.

CREATE TABLE IF NOT EXISTS marketplace_catalog_cache (
    digest          TEXT NOT NULL PRIMARY KEY,
    publisher_id    TEXT NOT NULL,
    expires_at      INTEGER NOT NULL,
    payload         TEXT NOT NULL,
    cached_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_marketplace_cache_cached_at
    ON marketplace_catalog_cache(cached_at);
CREATE INDEX IF NOT EXISTS idx_marketplace_cache_publisher
    ON marketplace_catalog_cache(publisher_id);