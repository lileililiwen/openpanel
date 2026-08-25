# Refine Host security with SSH keys

## Why

OpenPanel manages SSH public keys only for per-site jailed SFTP users
(`openspec/specs/sftp-jailed-shells/spec.md`, "SSH Public Key");
there is no lifecycle for host-level administrative keys
(`authorized_keys` of the admin account). Every peer panel treats
host SSH key management as baseline (CloudPanel SSH keys UI, KeyHelp,
Virtualmin), and passwordless admin access is the recommended hardening
path the compliance/CIS wizard points at.

## What Changes

- **Host key inventory**: register/remove SSH public keys for the
  panel's managed admin account with label, fingerprint, added-by,
  last-used timestamp.
- **authorized_keys writer**: renders a fully panel-owned
  `authorized_keys` file (managed block markers preserved for
  out-of-band lines), atomic write, 0600.
- **Options enforcement**: keys restricted with
  `no-agent-forwarding,no-port-forwarding` by default; command=
  restriction optional per key.
- **Audit + events**: every mutation audited; optional notification on
  new-key addition (stolen-key tripwire).
- Surfaces: API `/api/v1/host/ssh-keys`, CLI
  `openpanel host ssh-keys …`, web Host → SSH Keys page.

## Capabilities

### Modified Capabilities

- `host-security`: add host-level SSH public-key lifecycle to the
  existing firewall/login-abuse capabilities.

## Impact

- Domain: `HostSshKey{label, fingerprint, public_key, options,
  added_by, added_at, last_used_at}`, pure OpenSSH line parser/
  validator, `HostSecurityError::InvalidKey`.
- App: writer with managed-block markers + atomic rename; fingerprint
  computation via existing ssh-keygen shell-out precedent from
  sftp-jailed-shells; audit via AuditService.
- API/CLI/web as above; Admin role required.
- Security: private keys never accepted or stored; file permissions
  asserted post-write; no secrets in logs.
- Coupling: host-security module; sftp-jailed-shells (key parsing
  conventions); compliance (CIS references).

## Non-goals

- No root account management (panel admin account only).
- No SSH daemon config editing (host-security already owns ports).
- No certificate-based SSH CA issuance.
