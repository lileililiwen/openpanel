# mail Specification

## Requirements

## ADDED Requirements

### Requirement: Imported Mail Reuses Existing Mail Surfaces

Mail imported by a migration MUST map domains, mailboxes, and aliases onto the
existing mail surfaces instead of a parallel model. Imported credentials MUST
be carried only as typed secret references and MUST NOT be persisted or logged
in plaintext, and mail deliverability MUST be verified before any mail cutover.

#### Scenario: Imported mailbox maps to the existing mail surface

- **WHEN** a migration commits a domain's mailboxes and aliases
- **THEN** the imported mailboxes appear through the existing mail surfaces,
  their credentials are stored only as typed secret references, and a failing
  deliverability check blocks the mail cutover.
