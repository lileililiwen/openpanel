# Add Email deliverability monitoring

## Why

OpenPanel generates DKIM keys and enforces SPF/DMARC policies
(`openspec/specs/mail/spec.md`) but has zero visibility into actual
deliverability reputation: greps for `dnsbl|blacklist|deliverability|
reputation` return nothing across specs and crates. cPanel ships an
Email Deliverability UI; KeyHelp ships Rspamd reputation tooling;
mail-heavy hosts otherwise discover listing problems from angry users.

## What Changes

- **DNSBL monitoring**: scheduled + on-demand checks of configured
  blocklist zones for the host's outbound IPs and each mail domain's
  MX/A records; listings recorded with first-seen/last-seen.
- **Authentication audit**: parse-and-validate the published SPF,
  DKIM, and DMARC records per mail domain against the panel-managed
  values; drift reported.
- **DMARC aggregate report ingestion**: reports arriving at the
  domain's RUA mailbox are parsed into per-source pass/fail statistics
  (90-day retention).
- **Alerting**: new listings route through the existing notification
  channels; surfaces: API `/api/v1/mail/deliverability*`, CLI, web
  Mail → Deliverability tab.

## Capabilities

### New Capabilities

- `deliverability`: mail-domain reputation and authentication
  monitoring built on the mail capability's data.

## Impact

- Domain: `BlocklistCheck`, `Listing{zone, ip, first_seen, last_seen,
  resolved}`, `AuthAudit`, `DmarcSourceStat`; `DeliverabilityError`.
- App: `ResolverPort` (DNS TXT/A queries) implemented in app layer;
  cron-driven scheduler reusing existing job scope; report parser is
  pure (XML → stats); alerts via notifications dispatcher.
- API/CLI/web as above.
- Security: report XML parsed size-capped (≤10 MiB) with entity-free
  parsing; no message bodies retained — only aggregate statistics.
- Coupling: mail (domains/DKIM state), notifications, cron, dns
  (record reads).

## Non-goals

- No seed-list or inbox-placement testing (commercial data feeds).
- No automated delisting requests.
- No bounce/FBL feedback loop ingestion beyond DMARC aggregates.
