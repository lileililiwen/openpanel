-- mail-capture v0.1 initial schema.
-- Owned by the staging-mail-capture bounded context. Bodies are
-- stored AES-256-GCM encrypted (`nonce:ciphertext` hex) under the
-- panel master key - nothing here is secret-free plaintext body data.
-- The slot FK makes staging slot destruction purge every captured
-- message automatically (ON DELETE CASCADE).

CREATE TABLE IF NOT EXISTS mail_capture_messages (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL,
    slot_id TEXT NOT NULL REFERENCES staging_slots(id) ON DELETE CASCADE,
    mailbox TEXT NOT NULL,
    rcpt_to TEXT NOT NULL,
    subject TEXT NOT NULL,
    encrypted_body TEXT NOT NULL,
    captured_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mail_capture_site
    ON mail_capture_messages(site_id, captured_at);

CREATE INDEX IF NOT EXISTS idx_mail_capture_slot
    ON mail_capture_messages(slot_id);
