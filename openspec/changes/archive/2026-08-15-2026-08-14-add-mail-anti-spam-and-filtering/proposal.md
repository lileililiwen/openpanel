# Add Mail anti-spam and filtering

## Why

The archived `add-mail-hosting` change lists spam-filter administration,
Sieve, autoresponders, forwarding, catch-all, and mailing lists as
explicit Non-Goals. The active `refine-mail-with-dkim-spf-dmarc-defaults`
change adds DKIM/SPF/DMARC, quota, and sending-policy hardening but does
not cover these mailbox-level and domain-level mail features. Competitors
that OpenPanel benchmarks against — cPanel, Plesk, Panelica, and
HestiaCP — all ship a full mail stack including spam scoring, Sieve
filters, autoresponders, forwarding, catch-all, and mailing lists. This
change fills that gap so OpenPanel can offer a complete, competitive mail
product.

## What Changes

- SpamAssassin/Rspamd scoring with per-mailbox spam-folder routing.
- Greylisting at the domain/transport level.
- Sieve filter management (Pigeonhole-style) per mailbox.
- Per-mailbox autoresponders (with start/end window).
- Mail forwarding / aliases (per-domain forwarders).
- Catch-all alias per domain.
- Mailing lists (mailman-style) with moderation and subscription.
- New endpoints: `/mail/domains/{id}/antispam`,
  `/mail/mailboxes/{id}/filters` (sieve),
  `/mail/mailboxes/{id}/autoresponder`,
  `/mail/domains/{id}/forwarders`, `/mail/domains/{id}/catchall`,
  `/mail/lists`.
- Security: spam scores and headers SHALL never log message bodies;
  Sieve scripts are size-capped.

## Capabilities

### Modified Capabilities

- `mail`: add per-mailbox and per-domain spam filtering, Sieve,
  autoresponders, forwarding, catch-all, and mailing-list management.

## Impact

- Domain: `AntiSpamPolicy`, `GreylistEntry`, `SieveScript`,
  `AutoResponder`, `Forwarder`, `CatchAll`, `MailingList`.
- App: `MailFilterService`, `SieveCompiler`, `SpamScorer`,
  `MailingListService`.
- API/CLI/web: `/mail/...` endpoints above; CLI
  `openpanel mail {antispam,filter,autoresponder,forwarder,list}`; web
  Mail tabs.
- Security: spam scores/headers never log message bodies; Sieve scripts
  size-capped; autoresponder bodies are plaintext and never executed.
- Coupling: depends on `mail` capability; builds on the DKIM/SPF/DMARC,
  quota, and sending-policy work in `refine-mail-with-dkim-spf-dmarc-defaults`.
