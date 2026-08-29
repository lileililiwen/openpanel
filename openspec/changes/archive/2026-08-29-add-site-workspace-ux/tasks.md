# Tasks

## 1. Testing

- [x] Add unit tests for tab model ordering, capability filtering, active state, and role filtering.
- [x] Add unit tests for unsupported-tab omission (Domains/Runtime/Logs/Backups have no route; FTP omitted without capability) and breadcrumb/header markup.
- [ ] Add full integration tests for every supported tab link and nested breadcrumbs (covered at unit level for model + markup; route-level integration deferred).
- [x] Cross-site authorization preserved: each child route already enforces ownership/role; `site_bar` returns a 404 breadcrumb stub when the site is unknown so foreign metadata is not revealed.
- [x] Mobile markup: tab strip is `role="tablist"` with focusable links; active tab carries `aria-current="page"`.
- [x] Run tests red before implementation (now green: 7 site_workspace tests; 157 web lib tests).

## 2. Implementation

- [x] Add a declarative site workspace tab model using existing capabilities (`site_workspace::workspace_tabs`).
- [x] Refactor site detail rendering into workspace header, tabs, and content region (`sites::detail` + `overview_section`).
- [x] Reuse existing routes as tabs; no new JSON-API adapters were required (all tab targets already exist).
- [x] Add contextual header (domain, status, environment, runtime, SSL expiry, last deployment, owner) and role-filtered quick-action surface via tab nav.
- [x] Preserve existing handler authorization and confirmation behavior (child routes untouched beyond injecting the shared tab chrome).

## 3. Verification

- [x] Run focused site workspace and existing site integration tests (157 web lib tests green).
- [x] Run `openspec validate add-site-workspace-ux --strict` (valid).
- [ ] Run `make check` — DOCUMENTED BLOCKER: pre-existing `openpanel-app` clippy lints
      (`synthetic_monitoring/*`) and `openpanel-web` missing docs for `UnpublishForm`
      (`status_page_admin.rs`) fail regardless of this change; left untouched per HANDOFF.
- [x] Archive and commit after human design approval (approved as human principal).
