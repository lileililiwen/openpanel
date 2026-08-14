-- i18n v0.1 initial schema.
-- Owned by the i18n bounded context. Other modules MUST NOT
-- alter these tables. Translation store is signed; the panel
-- ships a default English catalog and a community catalog per
-- installed locale.

CREATE TABLE IF NOT EXISTS locale_user_prefs (
    user_id TEXT PRIMARY KEY,
    preferred_locale TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_locale_user_prefs_locale
    ON locale_user_prefs(preferred_locale);

CREATE TABLE IF NOT EXISTS locale_catalogs (
    locale TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    translator TEXT,
    context TEXT,
    signature TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (locale, key)
);

CREATE INDEX IF NOT EXISTS idx_locale_catalogs_locale
    ON locale_catalogs(locale);
