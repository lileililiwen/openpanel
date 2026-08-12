-- Identity v0.2: two-factor authentication (TOTP + recovery codes + pending-login challenges).
-- Phase A covers TOTP, recovery codes, and the login state machine. WebAuthn and
-- remember-device will append their tables in a follow-up migration.

CREATE TABLE IF NOT EXISTS user_factors (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL,
    verified_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT,
    last_used_step INTEGER,
    -- Encrypted-at-rest TOTP secret bytes (hex(nonce):hex(ciphertext)). NULL for non-TOTP factors.
    totp_secret_encrypted TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_factors_user ON user_factors(user_id);
CREATE INDEX IF NOT EXISTS idx_user_factors_kind ON user_factors(kind);

CREATE TABLE IF NOT EXISTS recovery_codes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    code_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    consumed_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_user ON recovery_codes(user_id);

CREATE TABLE IF NOT EXISTS login_challenges (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    source_ip TEXT,
    user_agent TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_login_challenges_user ON login_challenges(user_id);
CREATE INDEX IF NOT EXISTS idx_login_challenges_expires_at ON login_challenges(expires_at);
