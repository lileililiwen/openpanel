CREATE TABLE IF NOT EXISTS software_entries(
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    long_description TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL,
    kind TEXT NOT NULL,
    license TEXT NOT NULL,
    developer TEXT NOT NULL,
    homepage TEXT NOT NULL,
    icon TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    latest_version TEXT NOT NULL,
    search_text TEXT NOT NULL,
    activated_at TEXT NOT NULL,
    source_url TEXT NOT NULL,
    manifest_digest TEXT NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0,
    platforms_json TEXT NOT NULL DEFAULT '[]'
);

CREATE INDEX IF NOT EXISTS software_entries_category_idx ON software_entries(category);
CREATE INDEX IF NOT EXISTS software_entries_kind_idx ON software_entries(kind);
CREATE INDEX IF NOT EXISTS software_entries_search_idx ON software_entries(search_text);

CREATE TABLE IF NOT EXISTS software_entry_versions(
    entry_id TEXT NOT NULL,
    version TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    changelog_url TEXT,
    artifact_json TEXT,
    packages_json TEXT NOT NULL DEFAULT '[]',
    supports_php_json TEXT NOT NULL DEFAULT '[]',
    released_at TEXT,
    is_latest INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (entry_id, version),
    FOREIGN KEY (entry_id) REFERENCES software_entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS software_entry_tags(
    entry_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES software_entries(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS software_entry_tags_tag_idx ON software_entry_tags(tag);

CREATE TABLE IF NOT EXISTS software_entry_deps(
    entry_id TEXT NOT NULL,
    dep_id TEXT NOT NULL,
    PRIMARY KEY (entry_id, dep_id),
    FOREIGN KEY (entry_id) REFERENCES software_entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS software_entry_conflicts(
    entry_id TEXT NOT NULL,
    conflict_id TEXT NOT NULL,
    PRIMARY KEY (entry_id, conflict_id),
    FOREIGN KEY (entry_id) REFERENCES software_entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS software_refresh_history(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_url TEXT NOT NULL,
    attempted_at TEXT NOT NULL,
    outcome TEXT NOT NULL,
    manifest_digest TEXT,
    error TEXT
);
