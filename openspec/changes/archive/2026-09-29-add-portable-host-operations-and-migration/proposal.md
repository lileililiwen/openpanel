# Proposal: Add portable host operations and migration interoperability

## Why

Real-world adoption depends on operating more than one host and importing
existing sites safely. Virtualmin and other panels make domain-level backup,
restore, migration, quota, mail, and connectivity behavior explicit. OpenPanel
has agent fleet, backup/DR, and migration foundations, but needs a portable
operator workflow with dry-run plans, scoped changes, and source adapters.

## What Changes

- Define host registration, compatibility, drift, maintenance, and command
  safety workflows independent of SSH or a specific OS.
- Extend migration drivers for cPanel, aaPanel/Baota-style, and generic
  OpenPanel bundles with preview, conflict policy, and rollback.
- Add domain/site-level transfer verification, mail/DNS/SSL mapping, and
  post-migration checks.
- Keep the Mac host as one optional conformance target only.

## BFS Impact Map

- Capabilities: `terminal-host-fleet`, `monitoring-fleet-operations`,
  `migration-importers`, `backup-dr-operations`, `sites`, `mail`, `dns`, and
  `ssl` are modified.
- Users: operators, self-hosters, migration contributors, and site owners.
- Contracts/data: host profile, capability matrix, migration plan, conflict,
  verification, rollback, and evidence records.
- Integrations: mTLS agent, SSH/API adapters, DNS providers, mail providers,
  and external backup storage.
- Failure boundaries: incompatible source, missing feature, conflict,
  partial import, DNS/TLS mismatch, mail deliverability failure, and rollback.
- Unaffected: any one operator's local deployment repository or workstation.

## Capabilities

### New Capabilities

- `host-migration-interoperability`

### Modified Capabilities

- `terminal-host-fleet`
- `monitoring-fleet-operations`
- `migration-importers`
- `backup-dr-operations`
- `sites`
- `mail`
- `dns`
- `ssl`
- `quality` (the host-migration-interoperability contract gate)

## Non-goals

- No destructive import without explicit confirmation.
- No claim that every third-party panel feature maps losslessly.
- No automatic DNS cutover or mail-provider migration without verification.
- No requirement to run OpenPanel on macOS.
