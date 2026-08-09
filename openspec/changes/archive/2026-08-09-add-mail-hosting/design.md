## Context

Correct mail hosting requires SMTP submission, IMAP, TLS, DNS authentication, quotas, anti-abuse controls, and observable services. OpenPanel must never become an open relay or expose mailbox passwords/private DKIM keys.

## Goals / Non-Goals

**Goals:** managed domains/mailboxes/aliases, Postfix+Dovecot provisioning, TLS/DNS readiness, quotas, password rotation, diagnostics, backup integration, and safe RBAC.

**Non-Goals:** webmail, message reading/sending UI, bulk marketing, mailing lists, catch-all, spam-filter administration, calendars/contacts, or arbitrary MTA configuration.

## Decisions

1. Require dependency preflight: supported Postfix/Dovecot services, valid mail hostname, certificate, DNS capability/manual record confirmation, storage root, and port availability. Installation is outside this change.
2. Persist virtual domain/mailbox metadata in SQLite; hash login passwords using a Dovecot-compatible strong scheme and return generated plaintext once. DKIM private keys are encrypted at rest and written `0600`; public DNS values are metadata.
3. Generate isolated OpenPanel include files for Postfix/Dovecot, validate configs, atomically swap, reload, and rollback. Never edit entire administrator-owned configs.
4. Default-deny relay: unauthenticated relay outside local domains is always rejected; submission requires TLS and authentication. Apply per-mailbox/domain rate and quota limits.
5. DNS readiness checks MX, A/AAAA, PTR advisory, SPF, DKIM, and DMARC. Automatic changes use the DNS module's explicit proposals. TLS uses the SSL module without returning private keys.
6. Expose aggregate queue/delivery metadata only; never message subjects, bodies, or credentials. Backup hooks capture virtual mail storage and metadata consistently.

## Risks / Trade-offs

- Open relay/spam -> invariant tests, default-deny relay, submission auth/TLS, rate limits, and queue alerts.
- Poor deliverability -> readiness gate and explicit PTR/provider limitations.
- Data loss -> quotas, backup dependency, atomic config, and restore testing before general availability.

## Migration Plan

Ship dependency diagnostics first. Enabling mail requires explicit Owner confirmation after all mandatory checks pass; existing system mail configuration is never imported or overwritten.
