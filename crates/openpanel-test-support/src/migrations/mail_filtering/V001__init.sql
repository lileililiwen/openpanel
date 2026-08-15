-- Mail anti-spam and filtering: policies, greylist, sieve,
-- autoresponder, forwarders, catch-all, mailing lists.

CREATE TABLE IF NOT EXISTS antispam_policies (
    mailbox_id          TEXT PRIMARY KEY NOT NULL,
    spam_threshold      INTEGER NOT NULL,
    greylist_enabled    INTEGER NOT NULL DEFAULT 0,
    updated_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS greylist (
    sender              TEXT NOT NULL,
    recipient           TEXT NOT NULL,
    first_seen_at       TEXT NOT NULL,
    deferred_at         TEXT NOT NULL,
    whitelisted         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (sender, recipient)
);

CREATE TABLE IF NOT EXISTS sieve_scripts (
    mailbox_id          TEXT PRIMARY KEY NOT NULL,
    script              TEXT NOT NULL,
    last_compiled_at    TEXT
);

CREATE TABLE IF NOT EXISTS autoresponders (
    mailbox_id          TEXT PRIMARY KEY NOT NULL,
    enabled             INTEGER NOT NULL,
    body                TEXT NOT NULL,
    mode                TEXT NOT NULL,
    window_start        TEXT NOT NULL,
    window_end          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS forwarders (
    mailbox_id          TEXT NOT NULL,
    destination         TEXT NOT NULL,
    keep_local          INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mailbox_id, destination)
);

CREATE TABLE IF NOT EXISTS catch_all (
    domain                  TEXT PRIMARY KEY NOT NULL,
    destination_mailbox      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mailing_lists (
    address             TEXT PRIMARY KEY NOT NULL,
    members_json        TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL
);