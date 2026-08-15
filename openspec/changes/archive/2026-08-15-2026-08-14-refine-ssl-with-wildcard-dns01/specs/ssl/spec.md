## ADDED Requirements

### Requirement: Issue Wildcard Certificate via DNS-01

When a wildcard certificate is requested for a site, the system SHALL
issue a certificate covering the apex domain and `*.domain` using the
DNS-01 challenge, publishing the `_acme-challenge` TXT record through
the existing DNS provider port from the `dns` capability. The ACME
challenge responder SHALL listen only on `127.0.0.1:9080`, and the ACME
directory SHALL default to staging unless production is explicitly
selected. The TXT lease SHALL be revoked after the order is finalized,
whether the attempt succeeds or fails.

#### Scenario: Wildcard issuance succeeds

- **WHEN** an Owner requests `PUT /sites/{s1}/ssl` with
        `{ wildcard: true }`
- **THEN** a `_acme-challenge` TXT is published via the DNS provider,
        the issued cert SANs include `domain` and `*.domain`, the lease
        is revoked, and audit `WildcardCertIssued{domains}` records the
        names only.

#### Scenario: TXT lease revoked on failure

- **WHEN** DNS-01 finalization fails
- **THEN** the TXT lease is still revoked, no partial cert is stored,
        and the error is reported without leaking ACME account secrets.

#### Scenario: Staging by default

- **WHEN** no `mode` is supplied
- **THEN** the order is placed against the staging ACME directory, and
        the cert is marked `staging`.

### Requirement: Renew Wildcard Certificate via DNS-01

The system SHALL automatically renew a wildcard certificate using the
same DNS-01 path, creating and always revoking the `_acme-challenge`
TXT lease around finalization. Renewal failures SHALL be retried on the
existing schedule without leaving orphaned TXT records.

#### Scenario: Scheduled renewal reuses DNS-01

- **WHEN** the renewal scheduler fires for a wildcard cert
- **THEN** a fresh TXT lease is published, the order is finalized, and
        the lease is revoked; the new cert replaces the old bundle.

#### Scenario: Renewal failure cleans up

- **WHEN** a renewal attempt fails
- **THEN** the TXT lease is revoked and the prior valid cert is kept
        until the next retry.
