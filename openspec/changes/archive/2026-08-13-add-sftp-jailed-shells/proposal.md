# Add SFTP / jailed shells

## Why

`openspec/.../add-ftp-accounts` is in flight for FTP per site.
cPanel and Baota additionally ship **per-site SSH / SFTP** with
a chrooted shell so an operator can hand a developer a file-
upload key without giving them a real shell. OpenPanel's files
module is HTTP-only and chrooted through the panel; SSH/SFTP
is missing. This change adds a typed `jailed_shell` capability
mapped onto OpenSSH's `internal-sftp` + `ForceCommand` so a
user can be granted SFTP-only access scopped to one site root.

## What Changes

- New bounded context `sftp-jailed-shells` with the
  `SftpJailGrant` aggregate and `JailHostKeyService`.
- New endpoints: `POST /sites/{id}/sftp-jail`,
  `GET /sites/{id}/sftp-jail`, `DELETE /sites/{id}/sftp-jail`.
- New CLI: `openpanel site sftp {add,list,remove}`.
- New OpenSSH config managed by the panel: `/etc/ssh/
  openpanel.d/<site>.conf`. The panel refuses to edit any
  other OpenSSH configuration file.
- A typed `JailPublicKey` per grant (Ed25519 / RSA). Plaintext
  key is shown once on creation/rotation.

## Capabilities

### New Capabilities

- `sftp-jailed-shells`: per-site SSH/SFTP jails.

## Impact

- Domain: `SftpJailGrant`, `JailPublicKey`, `JailedShellStatus`.
- App: `JailHostKeyService`, configuration generator.
- API/CLI/web: `/sites/{id}/sftp-jail`; CLI; web panel.
- OS: only `/etc/ssh/openpanel.d/` is owned; the system
  `sshd_config` is untouched except for an `Include` line.
