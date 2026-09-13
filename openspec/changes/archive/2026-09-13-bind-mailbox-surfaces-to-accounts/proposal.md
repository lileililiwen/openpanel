# Proposal: Bind mailbox surfaces to accounts

## Why

OpenPanel contains mail-domain and filtering logic, but the webmail entry path
currently mints a fixed demo mailbox and several end-user operations are not
reachable through complete web/API/CLI surfaces. This prevents safe delegation
and makes the mail product appear functional when it is not account-bound.

## What Changes

- Bind mailbox sessions to authorized users, domains, and mailbox ownership.
- Add mailbox provisioning, quotas, aliases, forwarders, autoresponders, and
  Sieve/filter management surfaces.
- Add mailing-list member moderation and queue/deliverability status.
- Replace raw debug errors with safe localized error states.
- Audit all sensitive mail mutations and redact credentials/content.

## Capabilities

### Modified Capabilities

- `mail`
- `mail-filtering`
- `webmail-client`
- `identity`

## Non-goals

- Replacing the MTA.
- Full third-party webmail frontend rewrite.
- Reading or indexing message content beyond existing mailbox operations.

## Dependencies

Depends on `ratchet-quality-and-spec-maturity`; can proceed independently of
ACME after shared release governance is available.
