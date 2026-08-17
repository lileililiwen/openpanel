# Refine mail with DKIM / SPF / DMARC defaults and mailbox quotas

## Why

`openspec/specs/mail/spec.md` covers mail domain lifecycle, mailbox
and alias management, MTA configuration, and quota. It does not
mandate **defaults** for DKIM, SPF, and DMARC, and it does not pin
per-domain mailbox-level quota enforcement at the IMAP layer.
cPanel and Baota both ship with DKIM auto-generated on first enable,
SPF in the default DNS template (already covered by the DNS zone
template change), and a per-mailbox hard quota enforced by Dovecot /
Stalwart. Without these, an OpenPanel mail server is functional but
not parity-equivalent; spoofing is trivial and abusive mailboxes
saturate disk.

## What Changes

- New behavior in `mail::Domain::enable` that auto-generates an
  RSA-2048 DKIM keypair (Ed25519 supported) and publishes the
  public key as a DNS TXT record; private key encrypted at rest.
- New `MailboxQuota` value object and DAO storing per-mailbox
  quota in bytes; the IMAP/MDA integration enforces it on every
  delivery and returns `QuotaExceeded` to the SMTP client.
- New `DomainSendingPolicy` declaring per-domain outbound rate
  limit, max recipients per message, and SPF/DKIM/DMARC required
  status; defaults applied at enable.

## Capabilities

### Modified Capabilities

- `mail`: DKIM auto-generation; per-mailbox quota; outbound
  sending policy.

## Impact

- Domain: `DkimKeypair`, `MailboxQuota`, `DomainSendingPolicy`.
- App: `DkimGenerator`, `QuotaEnforcer` (in mail module,
  integrations with the MDA), `SendingPolicyEnforcer`.
- API/CLI/web: `GET /mail/domains/{id}/dkim`, `POST /mail/domains/{id}/dkim/rotate`,
  `PUT /mail/mailboxes/{id}/quota`, `openpanel mail quota`,
  web quota input on the mailbox detail page.
- Crypto: DKIM private key encrypted under master key; stored
  once per domain.
