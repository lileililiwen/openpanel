## Why

Email accounts are a major cPanel/aaPanel capability gap, but mail hosting has unusually high deliverability, abuse, DNS, storage, and service dependencies. It should follow DNS, services, security, backups, and logs as a deliberately scoped module rather than an incidental site feature.

## What Changes

- Add mail domains, mailboxes, aliases/forwarders, quotas, enable/disable, and password rotation.
- Provision Postfix and Dovecot through typed adapters and integrate TLS plus MX/SPF/DKIM/DMARC readiness.
- Add queue/delivery diagnostics, REST, CLI, web UI, audit, backup hooks, and abuse limits.
- Exclude webmail, marketing/bulk mail, mailing lists, catch-all, and message-content browsing from v1.

## Capabilities

### New Capabilities

- `mail`: secure hosted mail domain and mailbox administration.

### Modified Capabilities

None.

## Impact

Adds a bounded context, migrations, encrypted credentials, Postfix/Dovecot adapters, DKIM keys, DNS/SSL/services/backups/logs integration, and new network services. This change depends on the preceding platform capabilities.
