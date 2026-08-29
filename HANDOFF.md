# OpenPanel UI/UX Gap Roadmap Handoff

Updated: 2026-08-29  
Scope: planning artifacts only; no application-code implementation performed.

## Progress

Overall: `[█░░░░░░░░░] 1/7 implemented`

| Order | Change | Status | Depends on |
|---:|---|---|---|
| 1 | `repair-ui-discoverability` | `[x] implemented; archived & committed` | none |
| 2 | `add-audit-activity-center` | `[ ] proposed; design approval required` | 1 |
| 3 | `redesign-operations-dashboard` | `[ ] proposed; design approval required` | 1; audit links optional |
| 4 | `add-site-workspace-ux` | `[ ] proposed; design approval required` | 1 |
| 5 | `complete-file-database-backup-workflows` | `[ ] proposed; design approval required` | 1 |
| 6 | `add-terminal-and-host-fleet-ux` | `[ ] proposed; design approval required` | 1 |
| 7 | `strengthen-software-center-ux` | `[ ] proposed; design approval required` | 1 |

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

- `cargo test -p openpanel-web --lib` currently has a failing test: `nav_model::tests::every_nav_item_has_a_builtin_icon`; `Status page` references missing icon `pulse`.
- Current audit UI is intentionally a 501 stub in `crates/openpanel-web/src/audit.rs`.
- `CapabilitySet::shipped()` currently exposes only a limited set of capabilities in `crates/openpanel-web/src/layout.rs`.
- Existing UI styling tests cover token presence, forms, labels, breakpoints, and some shell behavior, but they do not prove complete route discoverability or full WCAG 2.2 behavior.
- The worktree already contains unrelated staged/untracked changes. Preserve them; stage only the selected change and its implementation paths.

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
