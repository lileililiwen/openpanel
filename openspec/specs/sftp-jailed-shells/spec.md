# sftp-jailed-shells Specification

## Purpose

TBD - created by archiving change add-sftp-jailed-shells.

## Requirements

### Requirement: SFTP Jail Grant

The bounded context SHALL model an `SftpJailGrant` carrying `site_id`, `owner_user_id`, `group_name` (unique `openpanel-sftp-<short_id>`), `jail_path` (canonical, absolute), `forced_command` (`internal-sftp -d <site_root_rel>`), `keys: Vec<JailPublicKey>`, `allow_password_fallback`, `allow_port_forwarding`, `status` (`Active | Disabled`), `created_at`, and `updated_at`.

#### Scenario: Relative jail path is rejected

- **WHEN** `SftpJailGrant::new` is called with a non-absolute jail path
- **THEN** construction fails with `SftpJailError::JailPathNotAbsolute`.

#### Scenario: Canonicalisation strips `.` and `..`

- **WHEN** the jail path is `/var/www/site/./content/../`
- **THEN** the resulting `jail_path` is `/var/www/site`.

### Requirement: SSH Public Key

The bounded context SHALL accept `JailPublicKey`s of type `ssh-ed25519` or `ssh-rsa`. The label must be non-empty. The fingerprint is a stable SHA-256-derived placeholder that the follow-on change replaces with a real SHA-256 hash.

#### Scenario: Unsupported key type is rejected

- **WHEN** a key with prefix `ssh-dss` is submitted
- **THEN** `JailPublicKey::new` returns `SftpJailError::InvalidKey`.

#### Scenario: Ed25519 key is accepted

- **WHEN** a key with prefix `ssh-ed25519` is submitted
- **THEN** the key is stored with label and fingerprint.

### Requirement: sshd Config Generator

The `render_sshd_config` function SHALL emit a `Match Group <group_name>` block that includes `ChrootDirectory`, `ForceCommand internal-sftp ...`, `AllowTcpForwarding <no|yes>`, `X11Forwarding no`, `PasswordAuthentication <no|yes>`, and `PermitTTY no`. Disabled grants SHALL be commented out with `# ` prefix.

#### Scenario: Disabled grants are commented out

- **WHEN** the grant's status is `Disabled`
- **THEN** the rendered config contains `# ChrootDirectory` and the rest of the block is commented out.

#### Scenario: Active grants enforce hardened options

- **WHEN** the grant is active
- **THEN** the rendered config contains `PermitTTY no`, `X11Forwarding no`, and `AllowTcpForwarding no` (default).

### Requirement: Ownership Boundaries

The panel SHALL only write under `/etc/ssh/openpanel.d/`. The system `sshd_config` SHALL be untouched except for an `Include` line the operator (or the panel installer) adds at first run.

#### Scenario: Writes stay inside the drop-in directory

- **WHEN** the panel persists grant configuration
- **THEN** every write lands under `/etc/ssh/openpanel.d/` and the
        base `sshd_config` remains byte-identical. The `SSHD_INCLUDE_DIR` constant is the canonical location.

### Requirement: Audit and Event Surface

The follow-on implementation SHALL emit audit events for grant creation, key add/remove, and grant disable/remove with the actor, the site id, and the affected grant id.

#### Scenario: Key removal is audited

- **WHEN** a key is removed from a grant
- **THEN** the audit event records the grant id, site id, and actor
        without key material. The bounded context as archived today owns the typed model + the config generator; the audit + service layer ships in the follow-on change.
