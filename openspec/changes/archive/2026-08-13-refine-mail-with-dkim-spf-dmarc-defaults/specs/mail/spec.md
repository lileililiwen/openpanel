## ADDED Requirements

### Requirement: DKIM Auto-Generation on Domain Enable

The system SHALL auto-generate a `DkimKeypair` for every mail domain on `enable`. The default algorithm SHALL be Ed25519; if the active MTA does not support Ed25519 the panel SHALL fall back to RSA-2048 and record `algo_used` on the `DkimKeypair`. The private key SHALL be encrypted at rest under the master key; only the public key is published to local DNS or rendered as a `RecordSuggestion` to a third-party provider.

#### Scenario: Enable succeeds with Ed25519

- **WHEN** `MailService::enable_domain` runs against an MTA that supports Ed25519
- **THEN** a `DkimKeypair{algo=ed25519}` row exists, an audit `DkimGenerated` is recorded, and a `RecordSuggestion` of kind `TXT` with name `<selector>._domainkey.<domain>` is queued.

#### Scenario: Ed25519 unsupported, RSA fallback

- **WHEN** the MTA reports Ed25519 unsupported at signing time
- **THEN** a `DkimKeypair{algo=rsa2048}` row exists with the same domain and the audit notes `fallback_to_rsa2048`.

### Requirement: DKIM Key Rotation With Grace

The system SHALL support rotating a domain's DKIM keypair. Rotation SHALL keep the old key valid for a configurable grace period (default 7 days) and SHALL atomically swap DNS and MTA configuration at rotation start. The grace window SHALL be enforced by the panel; MTA accepts both selectors as long as the grace window has not expired.

#### Scenario: Rotation grace honored

- **WHEN** an Owner calls `POST /mail/domains/{id}/dkim/rotate`
- **THEN** a new `DkimKeypair` row appears, `previous_key` is set, the MTA accepts both selectors, and an audit `DkimRotated` is recorded with the grace window end timestamp.

#### Scenario: Grace expiry

- **WHEN** the grace window closes
- **THEN** the MTA refuses signatures under the old selector with `550 5.7.7 signature expired`.

### Requirement: Per-Mailbox Quota Enforcement

The system SHALL persist per-mailbox `MailboxQuota{bytes_limit}` and SHALL enforce it at the MDA integration. A delivery whose `bytes_used + message_size > bytes_limit` SHALL be rejected with SMTP `550 5.2.2 Mailbox quota exceeded`, the offending message SHALL NOT be appended, and the audit log SHALL record `MailboxQuotaExceeded{mailbox_id, requested_bytes}` with the size only.

#### Scenario: Mailbox within quota

- **WHEN** a delivery fits within quota
- **THEN** the message is stored and the cached `bytes_used` increases by `message_size`.

#### Scenario: Boundary rejection

- **WHEN** a delivery would put `bytes_used` strictly above `bytes_limit`
- **THEN** delivery is rejected and no audit plaintext body is recorded.

#### Scenario: Quota scanner reconciles drift

- **WHEN** the nightly scan finds `bytes_used` diverged from the actual mailbox store by more than 1%
- **THEN** `bytes_used` is recomputed and an audit `MailboxQuotaReconciled` is recorded.

### Requirement: Domain Sending Policy

The system SHALL persist a `DomainSendingPolicy` per mail domain with at minimum: `max_recipients_per_message`, `max_outbound_per_hour`, `require_spf_aligned`, `require_dkim_signed`, `require_dmarc_aligned`. The MTA SHALL reject outbound SMTP that violates any policy with `550 5.7.1 Sender policy violated{reason}` and audit the rejection with the redacted reason.

#### Scenario: Unsigned outbound rejection

- **WHEN** an outbound message is sent through a domain with `require_dkim_signed=true` and the message lacks a DKIM signature
- **THEN** the MTA rejects with `5.7.1` reason `dkim_missing` and the audit `OutboundPolicyRejected{reason="dkim_missing"}` is recorded.

#### Scenario: Recipient cap rejection

- **WHEN** an outbound message has more than `max_recipients_per_message` recipients
- **THEN** the MTA rejects with reason `recipient_cap`.

### Requirement: Default Sending Policy at Enable

The system SHALL apply a default `DomainSendingPolicy` when a domain is enabled. The default SHALL be: `max_recipients_per_message=50`, `max_outbound_per_hour=500`, `require_spf_aligned=true`, `require_dkim_signed=true`, `require_dmarc_aligned=false`. Owners may tighten or relax these values via `PUT /mail/domains/{id}/sending-policy`, never loosen below the global minimum if one is configured.

#### Scenario: Default applied

- **WHEN** `MailService::enable_domain` completes
- **THEN** the domain's `sending_policy_id` is non-null and matches the default.

#### Scenario: Strict global minimum overrides lenient Owner input

- **WHEN** the configured global minimum is `require_dmarc_aligned=true` and the Owner submits `require_dmarc_aligned=false`
- **THEN** the policy update is rejected with `policy_below_global_minimum` and the prior policy remains in force.
