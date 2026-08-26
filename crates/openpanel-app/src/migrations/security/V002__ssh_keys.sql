CREATE TABLE IF NOT EXISTS host_ssh_keys (
  id TEXT PRIMARY KEY,
  label TEXT NOT NULL,
  fingerprint TEXT NOT NULL UNIQUE,
  public_key_b64 TEXT NOT NULL,
  algo TEXT NOT NULL,
  added_by TEXT NOT NULL,
  added_at TEXT NOT NULL,
  last_used_at TEXT
);
