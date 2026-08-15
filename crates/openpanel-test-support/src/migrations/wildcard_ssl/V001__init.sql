-- Wildcard SSL with DNS-01: cert requests and TXT leases.

CREATE TABLE IF NOT EXISTS wildcard_cert_requests (
    id              TEXT PRIMARY KEY NOT NULL,
    site_id         TEXT NOT NULL,
    apex            TEXT NOT NULL,
    wildcard        INTEGER NOT NULL,
    challenge       TEXT NOT NULL,
    endpoint_mode   TEXT NOT NULL,
    dns_provider    TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wildcard_cert_requests_site
    ON wildcard_cert_requests (site_id);

CREATE TABLE IF NOT EXISTS dns_leases (
    id                TEXT PRIMARY KEY NOT NULL,
    cert_request_id   TEXT NOT NULL,
    fqdn              TEXT NOT NULL,
    value             TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    revoked_at        TEXT,
    FOREIGN KEY (cert_request_id) REFERENCES wildcard_cert_requests(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_dns_leases_fqdn
    ON dns_leases (fqdn);
