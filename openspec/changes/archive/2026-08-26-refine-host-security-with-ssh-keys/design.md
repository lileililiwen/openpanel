# Refine Host security with SSH keys — Design

## Explore & Reuse

- `openspec/specs/sftp-jailed-shells/spec.md` "SSH Public Key" —
  existing key parsing/fingerprint conventions and ssh-keygen
  shell-out precedent in `crates/openpanel-app/src/sftp_jailed_shells/`;
  reused for parsing, not duplicated.
- `openspec/specs/host-security/spec.md` — Managed Firewall Rules /
  Login Abuse Protection: this adds a third section to the same module
  (`crates/openpanel-app/src/host_security/`), same surfaces pattern.
- Audit via `AuditService`; notification event via existing dispatcher.
- Atomic-file-write precedent: nginx vhost writer in
  `crates/openpanel-app/src/sites/` (temp + rename + permission assert).

## Model

```rust
pub struct HostSshKey {
    id: Uuid, label: String,
    fingerprint: String,          // SHA256:... from ssh-keygen
    public_key_b64: String,       // stored body only; options re-derived
    added_by: UserId, added_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}
pub enum KeyAlgo { Ed25519, EcdsaP256, Rsa3072Up }
```

Validation is pure: single line, known algorithm, base64 decodes,
minimum size per algo, no embedded options field.

## Rendering

```
# BEGIN openpanel-managed (do not edit)
<options> <algo> <b64> openpanel:<label>
...
# END openpanel-managed
<out-of-band lines preserved verbatim>
options = restrict-ish minimal set:
  no-agent-forwarding,no-port-forwarding[,command="…"]
write: tmpfile(0600) -> fsync -> rename -> stat assert
```

## Last-used matcher

Background task tails the SSH auth log source already allowed by
`openspec/specs/logs/spec.md` ("Authorized Log Sources"), matches
`Accepted publickey for <admin> .* SHA256:<fp>` lines, updates rows.
Bounded read discipline reused.

## Endpoints / CLI / Web

```
GET/POST/DELETE /api/v1/host/ssh-keys[/{id}]     (Admin only)
CLI: openpanel host ssh-keys {list,add,remove}
Web: Host → SSH Keys page
```

## Layering

Domain: pure types/parser/renderer. App: repo, writer, matcher,
service. Adapters standard; composition root unchanged (module exists).
