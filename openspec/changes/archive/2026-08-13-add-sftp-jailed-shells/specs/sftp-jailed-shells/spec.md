## Purpose

Provides operators a way to grant per-site SFTP access using
OpenSSH's `internal-sftp` and `ChrootDirectory` machinery,
without exposing a real shell. The panel writes ONLY to its
dedicated sshd `Include` directory; it never touches the
system-wide configuration.

# sftp-jailed-shells Specification

## Requirements

### Requirement: Dedicated sshd Include Path

The system SHALL write its sshd configuration files ONLY under
`/etc/ssh/openpanel.d/`. The panel MUST refuse any I/O that
would touch another path under `/etc/ssh/`. The system
`sshd_config` MUST include the panel directory via a
single `Include` directive (added by the installer or by the
operator). The panel SHALL NOT modify `sshd_config` after
install.

#### Scenario: Refuse to write sshd_config

- **WHEN** the panel is asked to write a file outside
        `/etc/ssh/openpanel.d/`
- **THEN** the request fails with `SftpJailError::PathOutsidePanelOwnedConfig`.

#### Scenario: Valid file accepted

- **WHEN** the panel writes `/etc/ssh/openpanel.d/<site>.conf`
- **THEN** the write succeeds; `sshd -t -f /etc/ssh/sshd_config`
        returns 0; audit `SshdConfigWritten` records the site id.

### Requirement: Per-Site Grant

The system SHALL let an authorised caller grant per-site SSH /
SFTP access. Each grant carries a list of `JailPublicKey`s, a
chroot path (`site.document_root` canonicalised once at
session start), a `forced_command` of `internal-sftp -d
<chroot_rel>`, and `AllowTcpForwarding=no`, `X11Forwarding=no`,
`PasswordAuthentication=no`, `PermitTTY=no` defaults. The
grant group name is `openpanel-sftp-<site_id>` and SHALL be
unique across the panel.

#### Scenario: Create grant with two keys

- **WHEN** an Owner posts two public keys against a grant for
        site `s1`
- **THEN** the generator writes a `Match Group openpanel-sftp-s1` block with both keys listed under `AuthorizedKeysFile`.

#### Scenario: chroot canonicalisation

- **WHEN** `site.document_root = /var/www/example.com/public_html/`
- **THEN** the generated `ChrootDirectory` is `/var/www/example.com/public_html` (no trailing slash).

### Requirement: Key Rotation

The system SHALL support adding and removing keys via
`POST /sites/{id}/sftp-jail/keys` and
`DELETE /sites/{id}/sftp-jail/keys/{label}`. Ed25519 and
RSA-2048/4096 keys are accepted; DSA and other deprecated
algorithms are rejected at parse time.

#### Scenario: Add a key

- **WHEN** an Owner posts a valid Ed25519 public key
- **THEN** the key is appended, sshd is reloaded, and audit `SftpJailKeyAdded` records the label only.

#### Scenario: Remove the last key

- **WHEN** an Owner removes the last key from a grant
- **THEN** the grant's group is removed from the panel's
        configuration file; the file is rewritten atomically;
        sshd is reloaded.

### Requirement: Remove Grant

`DELETE /sites/{id}/sftp-jail` SHALL remove the panel-written
file for that site atomically and reload sshd. A grant whose
file cannot be removed cleanly is reported as `RemoveFailed`
without partial state.

#### Scenario: Successful remove

- **WHEN** an Owner removes a grant and the file is removed
- **THEN** the file no longer exists and sshd reload returns 0.

#### Scenario: Concurrent remove is safe

- **WHEN** two operators race to remove the same grant
- **THEN** the first succeeds; the second receives `409 already_removed`.

### Requirement: Bounded Operations

The system SHALL refuse to operate when the panel's sshd
configuration files fail `sshd -t` syntax verification. The
panel MUST rotate to the last known-good file on syntax error
before any reload.

#### Scenario: Syntax error

- **WHEN** the panel-written file fails `sshd -t`
- **THEN** the panel restores the previous file atomically and audit `SshdConfigRolledBack` is recorded; no further grants are accepted until the issue is resolved.
