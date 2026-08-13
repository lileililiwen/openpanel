CREATE TABLE IF NOT EXISTS waf_rule_sets (
    site_id TEXT PRIMARY KEY NOT NULL,
    document_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS waf_hits (
    site_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    action TEXT NOT NULL,
    count INTEGER NOT NULL CHECK (count >= 0),
    last_triggered_at TEXT NOT NULL,
    PRIMARY KEY (site_id, rule_id),
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_waf_hits_site_time
    ON waf_hits(site_id, last_triggered_at DESC);
