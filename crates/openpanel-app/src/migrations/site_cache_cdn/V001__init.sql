-- Site cache and CDN integration v001: per-site cache policies,
-- CDN integrations, and the purge log.

CREATE TABLE IF NOT EXISTS site_cache_policies (
    site_id TEXT PRIMARY KEY NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    bypass_paths TEXT NOT NULL,
    static_assets_ttl_seconds INTEGER NOT NULL,
    keyed_cookies TEXT NOT NULL,
    stale_while_revalidate INTEGER NOT NULL DEFAULT 0,
    revalidation_required INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cdn_integrations (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('cloudflare', 'cloudfront', 'generic_http')),
    config_enc TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cdn_purge_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    integration_id TEXT NOT NULL REFERENCES cdn_integrations (id) ON DELETE CASCADE,
    purged_json TEXT NOT NULL,
    at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cdn_purge_log_integration
    ON cdn_purge_log (integration_id, at DESC);