CREATE TABLE IF NOT EXISTS deliverability_listings (
  ip TEXT NOT NULL,
  zone TEXT NOT NULL,
  first_seen TEXT NOT NULL,
  last_seen TEXT NOT NULL,
  resolved_at TEXT,
  PRIMARY KEY (ip, zone)
);
CREATE TABLE IF NOT EXISTS dmarc_source_stats (
  day TEXT NOT NULL,
  source_ip TEXT NOT NULL,
  messages INTEGER NOT NULL,
  dkim_pass INTEGER NOT NULL,
  spf_pass INTEGER NOT NULL,
  PRIMARY KEY (day, source_ip)
);
