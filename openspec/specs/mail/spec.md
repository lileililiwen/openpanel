# mail Specification

## Purpose
TBD - created by archiving change add-mail-hosting. Update Purpose after archive.
## Requirements
### Requirement: Mail Readiness and Domain Lifecycle

Owners SHALL run readiness checks and create, list, enable, disable, and delete mail domains only when supported mail services, hostname, TLS, storage, ports, and required DNS records are valid or explicitly acknowledged where external. Domain deletion SHALL require confirmation and report dependent mailboxes/aliases/backups.

#### Scenario: Missing MX record

- **WHEN** an Owner attempts to enable a mail domain whose MX does not target the configured mail host
- **THEN** enablement is blocked with the expected public record and no MTA reload occurs

#### Scenario: Domain deletion has mailboxes

- **WHEN** an Owner requests deletion of a domain with mailboxes
- **THEN** the system returns dependency counts and requires a separate short-lived destructive confirmation

### Requirement: Mailboxes and Aliases

Authorized callers SHALL create, list, update quota, enable, disable, rotate password, and delete mailboxes, and SHALL manage non-looping aliases/forwarders for owned domains. Passwords SHALL meet policy, be stored as strong hashes, be returned only at creation/rotation, and never appear in later output or audit.

#### Scenario: Create mailbox

- **WHEN** an authorized caller creates `alice@example.com` within domain quota
- **THEN** the mailbox is provisioned and its generated password is returned exactly once

#### Scenario: Alias loop

- **WHEN** a proposed alias creates a direct or transitive forwarding cycle
- **THEN** validation rejects it before configuration changes

### Requirement: Safe Mail Service Configuration

The system SHALL generate isolated Postfix/Dovecot includes, validate candidate configuration, atomically apply and reload with rollback, enforce authenticated TLS submission, and reject unauthenticated relay to non-local domains. DKIM private keys SHALL be encrypted at rest, written mode `0600`, and never returned.

#### Scenario: Relay attempt

- **WHEN** an unauthenticated external client submits mail from a non-local sender to a non-local recipient
- **THEN** the MTA rejects relay regardless of domain configuration

#### Scenario: Candidate config invalid

- **WHEN** Postfix or Dovecot validation fails
- **THEN** active includes remain unchanged and services are not reloaded

### Requirement: Quotas, Abuse Controls, and Diagnostics

The system SHALL enforce mailbox/domain storage quotas and configurable outbound rate limits, and SHALL expose only aggregate queue/delivery status, service health, DNS/TLS readiness, and redacted errors. Message subjects, bodies, authentication secrets, and recipient lists MUST NOT be exposed.

#### Scenario: Mailbox exceeds quota

- **WHEN** delivery would exceed a mailbox quota
- **THEN** delivery is rejected with the configured temporary/permanent policy and an aggregate event is recorded

### Requirement: Mail Surfaces and Integration

REST, CLI, and `/mail` web surfaces SHALL provide readiness, domains, mailboxes, aliases, quotas, password rotation, status, and diagnostics with site-style ownership rules, CSRF, and audit. Backup/restore SHALL include selected virtual mail data and metadata; DNS/SSL/service integrations SHALL use their public ports.

#### Scenario: User lists mailboxes

- **WHEN** a User lists mailboxes
- **THEN** only mailboxes in that user's domains are returned with no password hashes or secrets

#### Scenario: Backup selected mail domain

- **WHEN** a backup plan selects a mail domain
- **THEN** mailbox storage and required metadata are captured consistently without plaintext credentials
