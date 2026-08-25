# Competitive Gap Analysis — OpenPanel vs. peer panels (2026-08)

Research basis: web/GitHub survey of cPanel/WHM, Plesk Obsidian,
HestiaCP, CyberPanel, CloudPanel, aaPanel/BaoTa, 1Panel, Virtualmin,
ISPConfig, Froxlor, KeyHelp, Coolify, CapRover, Dokploy, Easypanel —
cross-checked against every capability in `openspec/specs/` and the
shipped code (`scripts/repo-map.sh`, targeted greps).

## Verdict summary

OpenPanel's spec coverage is already broad (60+ capabilities: sites,
databases, mail+filtering, DNS+DNSSEC, SSL incl. wildcard DNS-01,
backups+offsite targets, cron, WAF, docker, agent fleet, quotas,
billing export, i18n, themes, plugins+marketplace, AI-ops, Terraform
SDK, migration importers, synthetic monitoring, …). The gaps below are
what peers ship that OpenPanel does not yet spec or stub out in code.

## Confirmed gaps → proposed changes

| # | Gap | Who has it | Proposed change |
|---|-----|-----------|-----------------|
| 1 | Per-site HTTP controls: custom error pages, redirect rules, protected dirs (basic auth), hotlink protection, per-site IP allow/deny, MIME overrides, directory-index policy | cPanel, Plesk, Froxlor, CloudPanel, KeyHelp, aaPanel | `add-site-http-controls` |
| 2 | Browser web terminal | cPanel, Plesk, Hestia 1.9+, Virtualmin, aaPanel, 1Panel, Coolify, Easypanel | `add-web-terminal` |
| 3 | Mail end-user surfaces: autoresponder / forwarders / Sieve / mailing lists have domain+app code but no HTTP/web/CLI surface; outbound queue depth hardcoded to 0 | all full-mail panels | `refine-mail-with-user-surfaces` |
| 4 | Panel SSO (OIDC) + active-session inventory/revoke | Coolify (OIDC), cPanel (WebPros SSO); session mgmt is baseline | `refine-identity-with-sso-and-session-control` |
| 5 | Whole-server snapshot & host-to-host migrate | 1Panel snapshots, BaoTa machine migration | `add-server-snapshot-migration` |
| 6 | Preview deployments (ephemeral PR environments) | Coolify, Dokploy, Easypanel, CapRover — no classic panel has it | `add-preview-deployments` |
| 7 | Public status page publishing | native nowhere; differentiator on top of synthetic-monitoring | `add-status-page` |

## Ranked backlog

Items 1–7 below now have OpenSpec change folders (2026-08-25 batch);
only items 8–10 remain unspec'd.

1. **Per-site transport tuning** → `refine-sites-with-transport-tuning`
2. **DB remote-access enforcement** → `refine-databases-with-remote-access-enforcement`
   (prerequisite: `refine-specs-with-drift-repair`)
3. **App runtime env vars/secrets** → `refine-app-runtimes-with-env-secrets`
4. **Email deliverability monitoring** → `add-deliverability-monitoring`
5. **Backup restore drills** → `add-backup-restore-drills`
6. **Host SSH key lifecycle** → `refine-host-security-with-ssh-keys`
7. **Log rotation policy management** → `refine-logs-with-rotation-policy`
8. **Object storage hosting** — MinIO/S3-compatible server as a
   managed component (1Panel/aaPanel do it via app store). Could land
   as a software-center recipe instead of a native module.
9. **CalDAV/CardDAV, WebDAV** — rare (cPanel-only among verified);
   low priority, possible via recipes.
10. **Mailing-list moderation depth** — table stores members only;
    moderation hold/release flows from the archived delta are absent.

## Process finding (P0 hygiene)

Three shipped modules have archived spec deltas that were **never
folded into `openspec/specs/`**: `dnssec`/secondary-DNS,
`db_privileges`, `mail_filtering`. Code exists
(`crates/openpanel-app/src/lib.rs` exports their modules) but the live
specs do not — exactly the drift the quality spec's Spec-To-Test Drift
Gate exists to catch. Recommend a small `refine-specs-with-drift-repair`
change that re-runs `openspec archive` merges for those deltas before
new work lands on top of them (the mail change below depends on it).

## Sources

Competitor feature lists were compiled from vendor docs/release notes
(cpanel.net, docs.plesk.com, hestiacp.com, cyberpanel.net,
cloudpanel.io, aapanel.com, 1panel.pro, virtualmin.com, ispconfig.org,
froxlor.org, keyweb.de/keyhelp, coolify.io, caprover.com,
docs.dokploy.com, easypanel.io) and GitHub READMEs/changelogs, Aug 2026.
