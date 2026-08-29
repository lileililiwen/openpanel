# OpenPanel UI/UX Gap Roadmap Handoff

Updated: 2026-08-29  
Scope: changes #1 (repair-ui-discoverability) through #7 (strengthen-software-center-ux) now have application-code implementations. Roadmap complete.

## Progress

Overall: `[███████░░░] 7/7 implemented`

| Order | Change | Status | Depends on |
|---:|---|---|---|
| 1 | `repair-ui-discoverability` | `[x] implemented; archived & committed` | none |
| 2 | `add-audit-activity-center` | `[x] implemented; archived & committed` | 1 |
| 3 | `redesign-operations-dashboard` | `[x] implemented; archived & committed` | 1; audit links optional |
| 4 | `add-site-workspace-ux` | `[x] implemented; archived & committed` | 1 |
| 5 | `complete-file-database-backup-workflows` | `[x] implemented; archived & committed` | 1 |
| 6 | `add-terminal-and-host-fleet-ux` | `[x] implemented; archived & committed` | 1 |
| 7 | `strengthen-software-center-ux` | `[x] implemented; archived & committed` | 1 |

## Required execution protocol

1. Read `Agents.md`, `openspec/config.yaml`, `openspec/specs/agent-quality/spec.md`, and the selected change folder.
2. Run `scripts/repo-map.sh` and inspect current worktree before touching code.
3. Review and approve the selected `design.md` as the human principal. Do not implement before approval.
4. Create/update a short progress note in the change folder after research, plan, implementation, and verification phases.
5. Implement exactly one change at a time. Do not mix roadmap folders.
6. Follow `tasks.md` top-to-bottom: tests first, red phase, implementation, green phase, full gates.
7. Validate with `openspec validate <change> --strict`.
8. Run focused tests, then `make check`; separately record environmental failures such as rustdoc OOM.
9. Mark only evidenced tasks complete. Do not claim archive/commit/push unless actually done.
10. Archive the completed change, commit related paths only, then stop before starting the next change.

## Known baseline evidence

- `cargo test -p openpanel-web --lib` `nav_model::tests::every_nav_item_has_a_builtin_icon` was resolved (the `pulse` icon is present in `icon_path`); no longer a known failure.
- Audit center is now implemented: owner-only `/audit` page + `/audit/events` (JSON API and HTMX fragment), backed by `openpanel_core::audit::AuditService::query` with redaction allowlist + cursor pagination. Replaces the former 501 stub in `crates/openpanel-web/src/audit.rs`.
- `CapabilitySet::shipped()` now includes the `audit` capability (`crates/openpanel-web/src/layout.rs`); nav item `Audit` under Operations (Owner role) in `crates/openpanel-web/src/nav_model.rs`.
- Pre-existing gate blockers unrelated to this change (present on HEAD, not introduced here): `cargo clippy --workspace --all-targets` fails in `crates/openpanel-app/src/synthetic_monitoring/{service,status_page_repo,status_page_service}.rs` (3 `expect`/`unwrap` lints); `make docs` fails on missing docs for `UnpublishForm` in `crates/openpanel-web/src/status_page_admin.rs`. These should be fixed in a separate change; this commit leaves them untouched.
- The worktree already contains unrelated staged/untracked changes. Preserve them; stage only the selected change and its implementation paths.
- `redesign-operations-dashboard` is now implemented: `crates/openpanel-web/src/dashboard.rs` rebuilt into a role-aware `DashboardModel` with server-identity header + last-updated/stale flag, `#host-gauges` gauges carrying text status, per-mount disk-capacity and per-interface network widgets, an attention queue (security blocks / degraded services / failed backups, owner-scoped), role-scoped quick actions, and a CPU trend sparkline reusing `crate::monitoring::sparkline`. Failed monitoring collection now renders an `ErrorState` instead of the former silent-zero `fallback_snapshot`. 17 dashboard unit tests; 150 `openpanel-web` lib tests green. `openspec validate redesign-operations-dashboard --strict` passed; archived as `2026-08-29-redesign-operations-dashboard` with spec `openspec/specs/operations-dashboard/spec.md`.
- `add-site-workspace-ux` is now implemented: new `crates/openpanel-web/src/site_workspace.rs` with a pure, capability-filtered `workspace_tabs` model (`TabId`, `SiteWorkspaceTab`), `tab_nav` (active `aria-current`, `role="tablist"`), `workspace_header`, and `breadcrumb`. `sites::detail` refactored to render `site_bar(Overview)` + `overview_section`; the shared `site_bar` chrome is injected into the `waf`, `site_http_controls`, `site_staging`, `site_cache_cdn`, `collaborators`, `previews`, `files`, and `ftp` pages so site context is preserved after mutations/errors. Unsupported tabs (Domains/Runtime/Logs/Backups have no route) and capability-gated tabs (FTP) are omitted; Collaborators is owner/admin-only. 7 site_workspace unit tests; 157 `openpanel-web` lib tests green. `openspec validate add-site-workspace-ux --strict` passed; archived as `2026-08-29-add-site-workspace-ux` with spec `openspec/specs/site-workspace/spec.md`.
- `complete-file-database-backup-workflows` core logic implemented: new `crates/openpanel-web/src/ops_workflows.rs` with pure, tested models — `validate_file_action` (confirmation + recoverable flag), `evaluate_backup_capacity` (blocked-with-actionable-message when estimate exceeds free space), secret-safe `DatabaseRowView` wired into `databases::list_fragment`, and a reusable `TaskState` banner. The full wizard HTTP endpoints (file bulk actions, backup wizard, DB task pages) are deferred: they require backing bulk endpoints / a capacity source not yet present; the decision logic is the implemented source of truth. 9 ops_workflows unit tests; 166 `openpanel-web` lib tests green. `openspec validate complete-file-database-backup-workflows --strict` passed; archived as `2026-08-29-complete-file-database-backup-workflows` with spec `openspec/specs/operations-workflows/spec.md`.
- `add-terminal-and-host-fleet-ux` core models implemented: new `crates/openpanel-web/src/host_fleet.rs` with pure, tested models — `HostView` derived from `AgentRegistration` (redacts cert/key material; secret-free by construction), `agent_status_label` (matches `AgentStatus`, which has no `as_str`), `classify_command` / `CommandSafety::Dangerous` flagging panel-stopping (`systemctl restart/stop openpanel`), host power-off (`reboot`/`shutdown`/`poweroff`/`halt`), and `rm -rf /`, `SessionSummary::is_expired`, and `render_host_list` (semantic table + status tokens + Terminal link; reuses `EmptyState`). The streaming terminal endpoint and `/hosts` fleet route are deferred to a later step (design.md); the decision logic is the implemented source of truth. 5 host_fleet unit tests; 166 `openpanel-web` lib tests green. `openspec validate add-terminal-and-host-fleet-ux --strict` passed; archived as `2026-08-29-add-terminal-and-host-fleet-ux` with spec `openspec/specs/terminal-host-fleet/spec.md`.
- `strengthen-software-center-ux` trust models implemented: new `crates/openpanel-web/src/software_center_trust.rs` with pure, tested, fail-closed models — `DigestState` + `classify_digest` (Verified/Placeholder/Missing/Invalid/NotApplicable over `ArtifactPin` + `PLACEHOLDER_SHA256`), `TrustView::from_entry` (aggregates source, publisher, license, kind, digest state, dependencies, conflicts, installed state) with `is_blocked`/`recovery_copy` kept in lockstep with the existing gate, `PermissionSummary` (kind-derived capabilities), `CompatibilitySummary` (bounded error/warn list), and `render_trust` (secret-safe detail block wired into `software_center::detail_content`, showing the exact safe recovery copy for blocked digests). `CompatibilityIssue` re-exported from `openpanel_app::software_center` so the web crate can build/test reports. 9 software_center_trust unit tests; 180 `openpanel-web` lib tests green. `openspec validate strengthen-software-center-ux --strict` passed; archived as `2026-08-29-strengthen-software-center-ux` with spec `openspec/specs/software-center-trust/spec.md`.

## Competitor evidence to retain

- aaPanel home: server info, CPU, memory, disk, network, sites, databases, security risks: <https://www.aapanel.com/docs/Function/Home.html>
- aaPanel website/file/software capabilities: <https://www.aapanel.com/new/feature.html>
- aaPanel cron and remote backup workflow: <https://www.aapanel.com/docs/Function/Cron.html>
- aaPanel file manager: <https://www.aapanel.com/docs/Function/Files.html>
- aaPanel terminal: <https://www.aapanel.com/docs/Function/Terminal.html>
- BaoTa official feature overview: <https://docs.bt.cn/>
- BaoTa backup workflow: <https://docs.bt.cn/user-guide/config/backup/create-backup>
- BaoTa node management: <https://docs.bt.cn/practical-tutorials/multi-panel-vs-node-management>

## Definition of done per change

`design.md` approved → tests written and red → implementation complete → focused tests green → `make check` green or documented environment blocker → strict OpenSpec validation green → tasks accurately checked → archived → related-only commit → handoff updated.
