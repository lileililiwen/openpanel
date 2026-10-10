# Design: repair-home-discoverability

## Explore & Reuse (repo-map evidence, 2026-10-10)

`scripts/repo-map.sh` plus reads of `dashboard/mod.rs`,
`nav_model.rs`, `capability_registry.rs`, `operator_security.rs`, and
`software_center.rs` confirm every data source already exists; this
change composes them into dashboard regions only:

- `crates/openpanel-web/src/dashboard/mod.rs` — `DashboardModel` /
  `build_dashboard` (per-region isolation: gauges, `collect_counts`,
  `collect_alerts`, `build_attention`, `quick_actions_for`,
  `cards_section`, `attention_section`, `network_section`,
  `gauges_from_snapshot`). New widgets extend `ResourceCounts`
  (FTP count), add `SecurityRiskCard` / `SoftwareShortcut` /
  `TrafficSummary` / `PanelShortcut` / `QuotaSummary` view structs, and
  extend `cards_section` with links — never a second dashboard pipeline.
- Attention queue (`build_attention`): owner branch already aggregates
  `state.security.blocks`, `state.operator_security.attention_counts`,
  `state.system_services.inventory`, plus role-scoped backup failures.
  The security-risk card reuses `attention_counts`/`queue` with a
  one-click fix link to `/security/findings`; `seed_findings` stays the
  ingest policy. No scanner change.
- `OperatorSecurityService` (`operator_security.rs` handlers: `queue`,
  `find`, `preview`, `suppress_finding`, `remediate` + `seed_queue`):
  card counts and fix links delegate to the existing queue/detail
  routes. Evidence stays redacted per the control-plane spec.
- `SoftwareCenterService` (`software_center.rs`: `search`,
  `catalog_diagnostics`, `jobs`, `artifact_tasks`,
  `last_install_badges`): shortcuts list pinned entries via `search`;
  the Display-on-dashboard toggle is a per-user preference filter over
  that list, not a catalog mutation. No install-pipeline change.
- `CapabilitySet::shipped` (`layout.rs`) / `capability_registry.rs`
  (`entries`, `global_entries`, `find_by_route`, `role_may_see`,
  `template_matches`) / `nav_model.rs` (`NAV_SECTIONS`, `icon_svg`) /
  shell (`render_shell`): every dashboard link (sites, FTP, databases,
  security findings, software, monitoring, settings/services) MUST
  resolve via `find_by_route` to a registry entry and respect
  `role_may_see` (defense in depth: hidden link + handler denial).
  No new route is mounted; panel Update/Fix/Restart are deep-links.
- `tokens.css` vocabulary (`op-status-*`, `op-attn-*`, `op-empty-state`,
  `op-error-state`, `form`, `table`, `btn`, `card`, `badge`): all new
  cards/sections reuse these tokens; no new palette, no `px`, no
  `max-width` queries (mobile-first `min-width` only).

New code is limited to dashboard view-model extensions + render
functions in `dashboard/mod.rs` (with tests in `dashboard/tests.rs`
per the view/test split), a preference read for the software toggle,
and quota-summary projection over existing quota surfaces.

## DDD layering

- **Domain** (`openpanel-domain`): no new aggregate; toggle preference
  and quota-summary shapes are pure value objects (I/O-free) if needed.
- **App** (`openpanel-app`): no new service; reads go through
  `MonitoringService`, `OperatorSecurityService`, `SoftwareCenterService`,
  `FtpService`, quota surfaces. No repository or migration beyond a
  possible preference column.
- **Adapters** (`openpanel-web` only): `build_dashboard` extensions +
  maud sections with shared tokens; `WebUser` role scoping preserved
  (owner-only cards hidden for users, counts user-scoped). Failure in
  any region renders its error/empty state, never a silent zero.

## Widget map (Baota parity, existing sources)

1. Clickable Site/FTP/DB counts → `list_sites`, `FtpService::list`,
   `list_databases` → `/sites`, `/ftp`, `/databases`.
2. Security-risk card → `attention_counts` → one-click fix link to
   `/security/findings`.
3. Software shortcuts + Display-on-dashboard toggle → `search` + badges.
4. Traffic summary → aggregate of `network` snapshot rows.
5. Panel Update/Fix/Restart shortcuts → deep-links to existing routes.
6. Sub-user quota summary (QPS/bandwidth/traffic/disk) → existing quota
   surfaces, role-scoped.

Sibling non-overlap: `/migrate`, `/terminal`, `/ftp` page + phpMyAdmin,
file-manager affordances, and cron presets are owned by the five named
siblings; this change adds no route those siblings own.

---

*This design AWAITS human principal approval per Agents.md §8 item 10 — do NOT implement until approved.*
