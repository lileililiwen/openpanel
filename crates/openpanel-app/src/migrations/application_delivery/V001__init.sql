-- Application delivery: applications, environments, releases.
--
-- The bounded context is projection-first; the
-- `ApplicationDeliveryService` keeps the canonical state in
-- memory and the schema is here so a follow-on change can swap
-- the service for a SQLite-backed repository without a
-- re-merge. The four tables are intentionally simple: the
-- columns are bounded by the domain validators in
-- `openpanel_domain::application_delivery` (release digest <=
-- 128 chars; application name <= 64 chars; actor/confirmation
-- <= 254 chars; diagnostic <= 1024 chars).

CREATE TABLE IF NOT EXISTS application_delivery_applications (
    id            TEXT PRIMARY KEY NOT NULL,
    owner_id      TEXT NOT NULL,
    name          TEXT NOT NULL,
    source_kind   TEXT NOT NULL,
    source_url    TEXT NOT NULL,
    source_ref    TEXT NOT NULL,
    build_command TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL,
    UNIQUE (owner_id, name)
);

CREATE INDEX IF NOT EXISTS application_delivery_applications_owner_idx
    ON application_delivery_applications (owner_id);

CREATE TABLE IF NOT EXISTS application_delivery_environments (
    id              TEXT PRIMARY KEY NOT NULL,
    application_id  TEXT NOT NULL,
    kind            TEXT NOT NULL,
    name            TEXT NOT NULL,
    quota           INTEGER NOT NULL,
    secret_set_name TEXT NOT NULL,
    secret_set_version TEXT,
    created_at      TEXT NOT NULL,
    expires_at      TEXT,
    FOREIGN KEY (application_id) REFERENCES application_delivery_applications(id) ON DELETE CASCADE,
    UNIQUE (application_id, name)
);

CREATE INDEX IF NOT EXISTS application_delivery_environments_app_idx
    ON application_delivery_environments (application_id);

CREATE TABLE IF NOT EXISTS application_delivery_releases (
    id                   TEXT PRIMARY KEY NOT NULL,
    application_id       TEXT NOT NULL,
    environment_id       TEXT NOT NULL,
    digest_value         TEXT NOT NULL,
    digest_resolved_at   TEXT NOT NULL,
    source_kind          TEXT NOT NULL,
    source_url           TEXT NOT NULL,
    source_ref           TEXT NOT NULL,
    state                TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    last_transition_at   TEXT NOT NULL,
    previous_release_id  TEXT,
    diagnostic           TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (application_id) REFERENCES application_delivery_applications(id) ON DELETE CASCADE,
    FOREIGN KEY (environment_id) REFERENCES application_delivery_environments(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS application_delivery_releases_app_idx
    ON application_delivery_releases (application_id);
CREATE INDEX IF NOT EXISTS application_delivery_releases_env_idx
    ON application_delivery_releases (environment_id);
CREATE INDEX IF NOT EXISTS application_delivery_releases_state_idx
    ON application_delivery_releases (state);

CREATE TABLE IF NOT EXISTS application_delivery_audit (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at   TEXT NOT NULL,
    application_id TEXT NOT NULL,
    environment_id TEXT,
    release_id     TEXT,
    actor          TEXT NOT NULL,
    action         TEXT NOT NULL,
    state_before  TEXT,
    state_after   TEXT,
    diagnostic    TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS application_delivery_audit_app_idx
    ON application_delivery_audit (application_id);
CREATE INDEX IF NOT EXISTS application_delivery_audit_release_idx
    ON application_delivery_audit (release_id);
