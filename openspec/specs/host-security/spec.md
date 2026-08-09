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
