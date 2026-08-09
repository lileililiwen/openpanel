CREATE TABLE IF NOT EXISTS security_firewall_rules(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS security_login_failures(id INTEGER PRIMARY KEY AUTOINCREMENT,key TEXT NOT NULL,occurred_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_security_failures_key_time ON security_login_failures(key,occurred_at);
CREATE TABLE IF NOT EXISTS security_blocks(key TEXT PRIMARY KEY,payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS security_allowlists(network TEXT PRIMARY KEY);
