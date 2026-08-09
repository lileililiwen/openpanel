## ADDED Requirements

### Requirement: DNS Provider Accounts

Owners SHALL create, test, rotate, disable, and delete provider accounts with capability/permission discovery. Credentials MUST be encrypted at rest, accepted only over protected mutations, never returned, and redacted from errors, logs, audit, API, CLI, and web output.

#### Scenario: Add a valid provider token

- **WHEN** an Owner submits valid least-privilege credentials
- **THEN** encrypted credentials and discovered capabilities are stored and the plaintext is not returned

#### Scenario: Credential test fails

- **WHEN** a provider rejects credentials
- **THEN** a redacted provider diagnostic is returned and no plaintext credential is persisted in logs or audit

### Requirement: Zone Synchronization

The system SHALL list and synchronize accessible provider zones and records without deleting remote-only data. Each synchronized zone SHALL record provider identity, remote version, last success/error, and drift status.

#### Scenario: External record appears

- **WHEN** synchronization finds a remote record absent locally
- **THEN** the record is imported and marked synchronized without mutating the provider

### Requirement: Typed Record Lifecycle

Authorized callers SHALL create, list, update, and delete A, AAAA, CNAME, TXT, MX, CAA, NS, and SRV records subject to provider capability, DNS name/value/TTL validation, CNAME exclusivity, ownership, and optimistic remote-version checks.

#### Scenario: Concurrent remote edit

- **WHEN** a caller updates using a stale remote version
- **THEN** the system returns a conflict with refreshed metadata and does not overwrite the external edit

#### Scenario: CNAME conflicts with A

- **WHEN** a caller creates a CNAME at a name already holding an A record
- **THEN** validation rejects the mutation before contacting the provider

### Requirement: DNS Automation and Surfaces

REST, CLI, and `/dns` web surfaces SHALL expose provider accounts, zones, records, synchronization, mutation, and propagation checks with existing role/site ownership rules. Site record proposals and temporary DNS-01 TXT leases SHALL require explicit authorization; cleanup SHALL delete only the leased record.

#### Scenario: Create site record proposal

- **WHEN** site creation requests DNS automation for a configured zone
- **THEN** the caller sees the exact proposed records and provider before confirming mutation

#### Scenario: DNS-01 cleanup

- **WHEN** a DNS-01 workflow finishes
- **THEN** only its provider record identifier is deleted, even if other TXT values share the name
