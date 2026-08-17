# Add SFTP / jailed shells — Design

## sshd integration

`/etc/ssh/sshd_config` is patched by the operator (or by the
panel installer at first run) to add:

```
Include /etc/ssh/openpanel.d/*.conf
```

The panel OWNS and writes only files matching
`/etc/ssh/openpanel.d/<site>.conf`. Every other file under
`/etc/ssh/` is read-only to the panel.

## Grant shape

```
SftpJailGrant {
  site_id,
  owner_user_id,
  jail_path: PathBuf,        // site.document_root
  forced_command: "internal-sftp -d <jail>",
  keys: Vec<JailPublicKey>,
  allow_password_fallback: bool,
  allow_port_forwarding: bool,
  status: Active|Disabled
}
```

Per-site OpenSSH `Match Group` block:

```
Match Group openpanel-sftp-<site_id>
  ChrootDirectory <site_root_canonical>
  ForceCommand internal-sftp -d <site_root_rel>
  AllowTcpForwarding no
  X11Forwarding no
  PasswordAuthentication no
  PermitTTY no
```

## Endpoints

```
POST   /api/v1/sites/{id}/sftp-jail
  body: { public_keys: [{ key, label }], allow_password_fallback? }
GET    /api/v1/sites/{id}/sftp-jail
DELETE /api/v1/sites/{id}/sftp-jail
POST   /api/v1/sites/{id}/sftp-jail/keys   body: { key, label }
DELETE /api/v1/sites/{id}/sftp-jail/keys/{label}
```

## CLI

```
openpanel site sftp add     <site_id> --key <path> --label <l>
openpanel site sftp keys    <site_id>
openpanel site sftp remove  <site_id>
openpanel site sftp test    <site_id>   // sftp -o BatchMode=yes …
```

## Tests

```
1.1  Unit: sshd config generator; group-name uniqueness; key
      parser; chroot canonicalisation.
1.2  Property: every grant has a unique group; ChrootDirectory
      always canonical; PermitTTY never "yes".
1.3  Service tests: create / list / remove, key add / remove;
      refuses to write /etc/ssh/sshd_config.
1.4  Integration: live test sftp into a chrooted site using
      openssh sftp client against a fixture host key.
1.5  CLI E2E: grant + sftp upload + remove.
1.6  Web: site detail page exposes keys and grants (CSRF).
```
