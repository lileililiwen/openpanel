# Proposal: repair-home-discoverability

## WHY

Baota's home is the operator's map: clickable Site/FTP/DB counts, a
security-risk card with one-click fix, software shortcuts, a traffic
summary, panel Update/Fix/Restart shortcuts, and per-user quota usage.
OpenPanel's home (`crates/openpanel-web/src/dashboard/mod.rs`) shows
gauges, five static cards (Sites/Databases/Files/SSL/Users), alerts, and
an attention queue — but the counts are not all clickable, FTP has no
count at all, security findings need two clicks (`/security/findings`),
software has no home shortcut, traffic is per-interface rows with no
summary, panel self-management has no shortcut, and sub-user quota (QPS /
bandwidth / traffic / disk) is invisible. New operators land on a healthy
dashboard yet cannot discover where to act.

## WHAT

Repair home discoverability by composing existing services into the
dashboard: clickable Site/FTP/DB counts (FTP via `FtpService::list`),
a security-risk card bound to `OperatorSecurityService` open-findings
counts with a one-click fix link to `/security/findings`, software
shortcuts with a Display-on-dashboard toggle over `SoftwareCenterService`
search, a traffic summary aggregating existing network rows, panel
Update/Fix/Restart shortcuts as deep-links to existing settings/service
routes, and a sub-user quota summary (QPS/bandwidth/traffic/disk) from
existing quota surfaces. All widgets reuse `DashboardModel` /
`build_dashboard` regions with per-region failure isolation.

## Capabilities

- **Modified:** `operations-dashboard` — clickable counts, security-risk
  card, software shortcuts + toggle, traffic summary, panel shortcuts,
  quota summary (delta in this change).
- **Modified:** `capability-navigation` — dashboard deep-link targets
  resolve to registry entries; no new routes (delta in this change).
- Reused unchanged: `operator-security-control-plane` (findings queue),
  `software-center` (search/jobs), monitoring snapshot/history,
  `panel-navigation` sidebar, `web-ui-styling` tokens, identity/auth RBAC.

## Non-goals

- No new monitoring collection; traffic summary aggregates existing
  snapshot/history rows only.
- No new security scanner or finding type; the card projects existing
  `OperatorSecurityService` findings.
- No panel self-update engine; Update/Fix/Restart are deep-links to
  existing routes, not new executors.
- No sibling overlap: no `/migrate` wizard (see
  `add-migration-wizard-ui`), no `/terminal` surface (see
  `add-terminal-nav-and-saved-commands`), no `/ftp` page or phpMyAdmin
  link (see `add-ftp-global-and-phpmyadmin-link`), no file-manager or
  cron-preset changes (see `refine-files-with-baota-affordances`,
  `refine-cron-with-preset-types`).
