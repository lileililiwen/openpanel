## Why

A UX audit against aaPanel (verified source: `aaPanel/BTPanel/static/js/public.js`,
`layout.html`) and NN/g form-error heuristics (`nngroup.com/articles/errors-forms-design-guidelines/`)
found that OpenPanel's reported "interaction feels weird" stems from a **structural
gap**: the admin shell has no centralized interaction grammar. aaPanel funnels every
modal, toast, confirmation, tooltip, and load overlay through one component
(`layui.layer`) and reserves modals strictly for destructive actions; OpenPanel's
40+ route files each roll their own HTMX swap with no modal/toast surface, no
inline-validation hooks, and no feedback rail.

## What Changes

- Add a single HTMX + CSS **layer** (`crates/openpanel-web/src/layer.rs`) that
  owns `modal`, `confirm` (destructive actions only), `toast`, `tip`, and `load`
  fragments. `modals` are never used for validation errors.
- Add a server-side **inline-form-validation** contract (`crates/openpanel-web/src/forms.rs`)
  returning `422` with `{field: message}` + HTMX OOB fragment so errors render
  next to the offending field on blur.
- Add a reusable **ui-state-vocabulary** (`crates/openpanel-web/src/ui_states.rs`)
  — `EmptyState`, `LoadingState`, `ErrorState`, `NoResultsState` — used by every
  list route.
- Add a **feedback-widget** (`crates/openpanel-web/src/feedback.rs`) gated on
  `account_age_days >= 3` AND `localStorage.openpanel_feedback_seen` unset
  (mirrors aaPanel's `safe_day` + `NPS` localStorage pattern).
- Migrate every destructive action in `sites.rs`, `databases.rs`, `cron.rs`,
  `backups.rs`, `ssl.rs`, `files.rs`, `users.rs`, `dns.rs`, `ftp.rs`, `mail.rs`
  to `layer.confirm`.
- Mount `layer` and `feedback-widget` once in `crates/openpanel-web/src/layout.rs`.

## Capabilities

### New Capabilities

- `inline-form-validation`: per-field validation on blur, errors rendered inline
  via `hx-swap-oob`, `422` server contract with JSON map of field → message.
- `modal-toast-confirm-surface`: `GET /layer/{modal,confirm,toast,tip,load}`
  fragments; modals reserved for destructive actions, toasts for routine
  feedback.
- `ui-state-vocabulary`: `EmptyState` / `LoadingState` / `ErrorState` /
  `NoResultsState` reusable maud components.
- `feedback-widget`: NPS-style widget with `safe_day >= 3` and
  `localStorage.openpanel_feedback_seen` gating, backed by new
  `feedback` aggregate in `openpanel-domain`.

### Modified Capabilities

- `web-ui`: extend the shell to mount the layer root, the feedback-widget
  container, and the `htmx:beforeRequest` / `htmx:afterRequest` hooks that
  drive toasts. (Shell-level change only — does not alter navigation,
  login, CSRF, or session requirements.)

## Impact

- **Code**: 5 new files in `crates/openpanel-web/src/`, 2 new files in
  `crates/openpanel-app/src/`, 1 new file in `crates/openpanel-domain/src/`,
  1 new test file in `crates/openpanel-web/tests/`, CSS additions to
  `crates/openpanel-web/assets/{app,tokens}.css`. 10 existing route files
  migrate destructive actions.
- **Deps**: none — HTMX-only, no layui/React adoption.
- **API**: new internal endpoints `GET /layer/*` and `POST /feedback`; no
  breaking changes to existing JSON API.
- **Schema**: new SQLite migration `feedback` table in
  `crates/openpanel-app/migrations/`.

## Non-Goals

- Adopting layui, React, Vue, or any client-side framework (HTMX + maud only).
- Visual theme or design-token changes (owned by `web-ui-styling`,
  `themeable-ui`).
- Navigation restructure (owned by `panel-navigation`).
- Full Baota Panel parity (Baota-specific claims were unverified in the audit;
  defer until a Baota-focused round of research).
- Replacing the existing CSRF or session model (owned by `web-ui`).