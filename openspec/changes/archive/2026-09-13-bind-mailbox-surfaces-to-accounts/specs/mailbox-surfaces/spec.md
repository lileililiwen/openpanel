# mailbox-surfaces Specification

## Requirements

## ADDED Requirements

### Requirement: Mailbox Access Is Account-Bound

Every webmail session and mailbox operation MUST resolve through the
authenticated user and authorized domain/mailbox relationship.

#### Scenario: User enters webmail

- **WHEN** a user opens webmail
- **THEN** the minted session references an authorized mailbox owned by or
  delegated to that user, never a fixed demo mailbox.

### Requirement: End-User Mail Operations Are Complete

Authorized users MUST be able to manage mailbox settings, aliases,
forwarders, autoresponders, filters, and quotas through the declared adapters.

#### Scenario: Unauthorized forwarder mutation

- **WHEN** a user submits a forwarder for another account
- **THEN** the operation is rejected, audited as denied, and no row changes.

### Requirement: Mail Health Is Observable

The panel MUST expose bounded queue depth, oldest deferred message, channel
health, and last refresh/error state without exposing message content or
credentials.

#### Scenario: Queue provider unavailable

- **WHEN** queue inspection fails
- **THEN** the UI shows unavailable state and recovery guidance rather than zero.

### Requirement: Mail Mutations Are Safe

All browser mutations MUST validate CSRF, authorization, input limits, and
audit redacted field names.

#### Scenario: Valid mailbox update

- **WHEN** an authorized user submits a valid update with valid CSRF
- **THEN** the change persists, an audit event records the safe operation, and
  no password or message body is returned.
