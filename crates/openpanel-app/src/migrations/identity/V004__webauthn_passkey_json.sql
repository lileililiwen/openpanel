-- Identity v0.4: WebAuthn full-Passkey storage for assertion deserialization.
-- The passkey_json column holds the webauthn-rs `Passkey` serialized
-- via serde, so the assertion flow can reconstruct the COSE key without
-- needing to decode the raw bytes itself.

ALTER TABLE webauthn_credentials ADD COLUMN passkey_json TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS totp_enrollment_challenges (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    encrypted_secret TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_totp_enrollment_user
    ON totp_enrollment_challenges(user_id);
