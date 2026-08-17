# Refine mail with DKIM / SPF / DMARC defaults and mailbox quotas — Design

## DKIM auto-generation

```
enable(domain):
  keypair = DkimKeypair::generate(algo=Ed25519, fallback=Rsa2048)
  private_pem = encrypt_under_master(keypair.private_pem)
  insert dkim_keys(domain_id, public_pem, private_ciphertext, algo)
  publish to local DNS (if managed) or render into a RecordSuggestion
  audit DkimGenerated{domain_id, algo}
  reload mail services (postfix, opendkim, stalwart-reload)
```

Ed25519 is preferred where the MDA supports it (Stalwart does; the
spec MAY accept failures and fall back to RSA-2048).

## Mailbox quota

```sql
CREATE TABLE mailbox_quotas (
  mailbox_id TEXT PRIMARY KEY,
  bytes_used INTEGER NOT NULL DEFAULT 0,
  bytes_limit INTEGER NOT NULL,   -- bytes; 0 = "use domain default"
  updated_at INTEGER NOT NULL
);
```

The MDA integration (separate spec covers the MDA itself) MUST
call `quota_increment` on successful delivery and reject when
`bytes_used + message_size > bytes_limit`. The error MUST be
`550 5.2.2 Mailbox quota exceeded`. A periodic scanner reconciles
the cache against the Maildir / Stalwart mailbox store.

## Domain sending policy

```rust
struct DomainSendingPolicy {
    max_recipients_per_message: u16,    // default 50
    max_outbound_per_hour: u32,         // default 500
    require_spf_aligned: bool,          // default true
    require_dkim_signed: bool,          // default true
    require_dmarc_aligned: bool,        // default false (start permissive)
}
```

Policy is enforced by the MTA on outbound smtp. A rejected message
returns `550 5.7.1 Sender policy violated` and the panel audits
`OutboundPolicyRejected{reason}`.

## Endpoints

```
GET  /api/v1/mail/domains/{id}/dkim
PUT  /api/v1/mail/domains/{id}/dkim        body: { algo: "ed25519" | "rsa2048" }
POST /api/v1/mail/domains/{id}/dkim/rotate → rotates keypair; old key valid for 7d grace
GET  /api/v1/mail/mailboxes/{id}/quota
PUT  /api/v1/mail/mailboxes/{id}/quota     body: { bytes_limit: 0 = domain default }
GET  /api/v1/mail/domains/{id}/sending-policy
PUT  /api/v1/mail/domains/{id}/sending-policy
```

## Tests

```
1.1  Unit: DkimKeypair generation and rotation; quota math
     (bytes_used + msg_size > bytes_limit rejects).
1.2  Property: every Domain after enable has a DkimKeypair row;
     bytes_used is monotonic per mailbox; sending policy never
     lets an unsigned message through when require_* is true.
1.3  Service tests with mock MDA and MTA: DKIM rotation grace
     period, quota rejection, policy rejection, audit.
1.4  Integration: enabled domain → DKIM in DNS (mock provider);
     mailbox over quota → MDA returns 5.2.2; outbound policy
     rejected.
1.5  CLI E2E: `openpanel mail dkim rotate`; `openpanel mail quota set …`.
1.6  Web: DKIM public key display; quota input; policy editor.
```
