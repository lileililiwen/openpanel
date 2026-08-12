-- Identity v0.3: WebAuthn credentials and in-flight ceremony state.

CREATE TABLE IF NOT EXISTS webauthn_credentials (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    factor_id TEXT NOT NULL,
    -- Credential ID emitted by the authenticator (base64url-encoded
    -- when stored as text, opaque to the panel).
    credential_id TEXT NOT NULL UNIQUE,
    -- COSE-encoded public key, hex-encoded for storage.
    public_key_spki TEXT NOT NULL,
    -- Monotonically increasing counter for clone detection.
    sign_count INTEGER NOT NULL DEFAULT 0,
    -- Comma-separated transport hints ("usb","nfc","ble","internal").
    transports TEXT,
    -- Preferred user-verification policy ("required"|"preferred"|"discouraged").
    uv_policy TEXT NOT NULL DEFAULT 'preferred',
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_webauthn_credentials_user ON webauthn_credentials(user_id);

CREATE TABLE IF NOT EXISTS webauthn_challenges (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    -- Serialized ceremony state from webauthn-rs
    -- (`PasskeyRegistration` for registration,
    -- `PasskeyAuthentication` for assertion). The panel treats
    -- this as opaque JSON.
    state_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_webauthn_challenges_user ON webauthn_challenges(user_id);
CREATE INDEX IF NOT EXISTS idx_webauthn_challenges_expires_at ON webauthn_challenges(expires_at);