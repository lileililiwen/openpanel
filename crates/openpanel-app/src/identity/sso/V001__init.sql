CREATE TABLE IF NOT EXISTS sso_connections (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    issuer_url TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_cipher TEXT NOT NULL,
    default_role TEXT NOT NULL,
    auto_provision INTEGER NOT NULL DEFAULT 0,
    trust_idp_mfa INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS external_identities (
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    user_id TEXT NOT NULL,
    PRIMARY KEY (issuer, subject)
);

CREATE TABLE IF NOT EXISTS sso_states (
    state TEXT PRIMARY KEY,
    nonce TEXT NOT NULL,
    pkce_verifier TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
