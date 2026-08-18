# Progress

## Status (after implementation)

**Goal:** Add a centralized admin interaction surface: layer overlays,
inline form validation, reusable UI states, and a gated feedback widget.

**Approach:** DDD-shaped: domain `feedback` aggregate → app SQLite
service/module → web `layer`/`forms`/`ui_states`/`loading`/`feedback`
modules mounted in the shell, then destructive-action migration and a
list-route empty-state sweep.

**Done:**
- Domain: `crates/openpanel-domain/src/feedback.rs` (`FeedbackEntry`,
  `Sentiment`, `FeedbackError`, `FeedbackRepository`,
  `rate_limit_exceeded`, domain constants) + unit + proptest coverage.
- App: `crates/openpanel-app/src/feedback.rs` (SQLite repo, service with
  rolling 24h rate limit, module) + migration
  `crates/openpanel-app/src/migrations/feedback/V001__init.sql`; wired
  into `openpanel-cli` and `openpanel-test-support` composition roots.
- Web: `layer.rs` (`/layer/{modal,confirm,toast,tip,load}` fragments,
  CSRF embedding, destructive-action registry), `forms.rs`
  (`render_field_error`/`render_field_ok`, `validation_error_response`,
  `POST /forms/validate`), `ui_states.rs` + `loading.rs` (four state
  components + named hx-indicator), `feedback.rs` (widget template,
  age gate, `POST /feedback` with rate limit + toast trigger).
- Shell: `#layer-root`, `#form-errors`, `#feedback-widget-root`
  mounted in `layout.rs` with the `htmx:afterRequest` layer-toast hook
  and localStorage-gated widget hydration.
- Destructive actions migrated to `/layer/confirm` in `sites`,
  `databases` (delete + reveal), `cron`, `ssl`, `files`, `users`, `dns`,
  `ftp`. (`backups`/`mail` expose no destructive web action; `users`
  has no disable-2fa/revoke-token web endpoint; `dns` has no delete-zone
  web endpoint.)
- List-route sweep: empty lists render `EmptyState`/`NoResultsState`
  (sites, databases, cron, backups, ssl, files, users, dns, ftp, mail,
  api_tokens, monitoring, logs, notifications, dashboard alerts).
- CSS: `op-*` layer/ui-state/feedback classes in `app.css`, token-sourced.
- Tests: `tests/integration/admin_interaction_surface.rs` (14 tests) +
  unit tests across domain/app/web; existing `web_ui.rs` assertions
  updated to the layer-confirm contract.

**Known gaps (spec language broader than tasks):**
- The 422 JSON body contract is delivered by `validation_error_response`
  and the blur endpoint carries the error map in `X-Form-Errors` (the
  body is the OOB fragment HTMX needs). Full form handlers (e.g.
  `POST /sites`) still re-render 200-with-error; converting every form
  handler to the 422 contract is not enumerated in `tasks.md`.
- The "no form route hand-rolls its own error markup" requirement is
  satisfied for new code; existing per-route error regions remain.
- `openspec validate --strict` and `make check` are green. Human review
  of the change is an ongoing process outside the task list (per
  AGENTS.md); manual browser walkthrough happens outside CI.

**Blocker:** none; awaiting `make check` results and human review.
