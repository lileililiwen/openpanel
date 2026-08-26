# host-security Specification

## Purpose
TBD - created by archiving change add-host-security. Update Purpose after archive.
## Requirements
### Requirement: Managed Firewall Rules

The system SHALL manage only its dedicated nftables table and SHALL support list, preview, add, update, enable, disable, delete, and apply for typed inbound rules. Candidate rulesets MUST pass domain validation and `nft --check` before atomic apply; failure MUST restore the last-known-good OpenPanel ruleset.

#### Scenario: Apply a valid allow rule

- **WHEN** an Owner adds TCP port 443 from all sources and confirms the preview
- **THEN** the candidate is atomically applied and an audit event records rule ID and action

#### Scenario: Syntax check fails

- **WHEN** nftables rejects the candidate
- **THEN** active rules remain unchanged and a redacted diagnostic is returned

### Requirement: Lockout Prevention

The system SHALL protect configured panel and SSH access, require break-glass confirmation for a potentially disconnecting change, start a timed rollback watchdog, and provide console recovery instructions. It MUST NOT modify non-OpenPanel firewall tables.

#### Scenario: Last panel path would be denied

- **WHEN** a change would deny every configured panel access path without break-glass confirmation
- **THEN** apply is rejected before host mutation

#### Scenario: Risky change is not confirmed

- **WHEN** reachability confirmation is not received before the watchdog deadline
- **THEN** the last-known-good OpenPanel ruleset is restored

### Requirement: Login Abuse Protection

Login SHALL be throttled by normalized account and trusted client IP with configurable attempts, window, and bounded exponential block duration. Proxy-derived addresses SHALL be used only for configured trusted proxies. Responses SHALL not reveal whether an account exists.

#### Scenario: Repeated invalid password

- **WHEN** one account exceeds the configured failed-attempt threshold
- **THEN** subsequent attempts receive the same generic response with rate-limit metadata until the temporary block expires

#### Scenario: Forged forwarding header

- **WHEN** an untrusted peer sends `X-Forwarded-For`
- **THEN** throttling uses the peer address and ignores the header

### Requirement: Security Surfaces and Events

Owner-only REST, CLI, and `/security` web surfaces SHALL expose support/status, rule lifecycle, preview/apply/rollback, active blocks, unblock, allowlists, and a posture summary. Security events SHALL record actor, trusted source, action, target identifier, result, and timestamp without passwords or tokens.

#### Scenario: Admin attempts firewall mutation

- **WHEN** an Admin submits a firewall rule
- **THEN** the system returns forbidden and changes no rules

#### Scenario: Owner unblocks an address

- **WHEN** an Owner removes an active temporary IP block
- **THEN** the block ends immediately and the action is audited

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

