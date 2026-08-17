-- Themeable UI and white-label v001: per-account theme overrides
-- keyed by owner user id, with a secondary index on the
-- panel_domain fqdn for host-header lookups.

CREATE TABLE IF NOT EXISTS theme_overrides (
    owner_id TEXT PRIMARY KEY NOT NULL,
    brand_name TEXT NOT NULL,
    logo_path TEXT,
    palette_json TEXT NOT NULL,
    typography_json TEXT NOT NULL,
    panel_domain_fqdn TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_theme_overrides_fqdn
    ON theme_overrides (panel_domain_fqdn)
    WHERE panel_domain_fqdn IS NOT NULL;
