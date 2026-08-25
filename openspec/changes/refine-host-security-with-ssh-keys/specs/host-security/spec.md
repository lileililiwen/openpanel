## ADDED Requirements

### Requirement: Host SSH Key Inventory

Admins SHALL register labelled SSH public keys for the panel-managed
admin account. The system SHALL accept only single-line public keys of
allowed algorithms (ed25519, ecdsa-p256, RSA ≥3072), reject embedded
options or multi-line input, reject duplicate fingerprints, and record
added-by/added-at metadata.

#### Scenario: Valid key registered

- **WHEN** an Admin submits a valid ed25519 public key with label
        "laptop"
- **THEN** the key is stored with its computed fingerprint and an
          audit event records the fingerprint only.

#### Scenario: Weak or malformed rejected

- **WHEN** the submitted material is an RSA-1024 key or contains
        embedded options
- **THEN** registration fails with `InvalidKey` and nothing is
        persisted.

### Requirement: Managed authorized_keys Rendering

The system SHALL render the admin account's `authorized_keys` as a
panel-managed block containing exactly the registered keys with the
enforced restriction options, preserving out-of-band lines outside the
block, writing atomically with mode 0600, and leaving the previous
file untouched on write failure.

#### Scenario: Out-of-band lines preserved

- **WHEN** the file already contains a manually added non-panel line
        and a key is added via the panel
- **THEN** the manual line survives below the managed block unchanged.

#### Scenario: Failed write is inert

- **WHEN** the atomic write fails mid-way
- **THEN** the original file content and permissions are unchanged.

### Requirement: Key Usage Visibility

The system SHALL track last-used timestamps per fingerprint from SSH
authentication logs and expose them in the inventory surfaces.

#### Scenario: Login updates timestamp

- **WHEN** a successful publickey login matches a stored fingerprint
- **THEN** that key's `last_used_at` updates on the next matcher run.

### Requirement: Key Surfaces and Audit

Admins SHALL manage host keys via API, CLI, and web; every mutation
SHALL be audited with fingerprints only — never full key bodies — and
an optional new-key notification SHALL be available through existing
channels.

#### Scenario: Removal audited

- **WHEN** an Admin removes a key
- **THEN** the rendered block updates and the audit trail names the
          removed fingerprint.
