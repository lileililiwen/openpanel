-- Plugin extension framework initial schema (V001)
-- Records every installed plugin.
-- Capability gating is enforced at the supervisor. This table
-- only persists the install record.

CREATE TABLE IF NOT EXISTS installed_plugins (
    id              TEXT NOT NULL PRIMARY KEY,
    version         TEXT NOT NULL,
    publisher       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN
                      ('installed', 'enabled', 'disabled', 'failed')),
    installed_at    TEXT NOT NULL,
    enabled_at      TEXT,
    last_error      TEXT
);

CREATE INDEX IF NOT EXISTS idx_installed_plugins_status
    ON installed_plugins(status);

CREATE INDEX IF NOT EXISTS idx_installed_plugins_publisher
    ON installed_plugins(publisher);