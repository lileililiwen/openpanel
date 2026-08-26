## 1. Testing

- [x] 1.1 Add the interaction-surface integration test file with failing
      integration tests covering all four specs: inline-form-validation
      (422 contract + blur OOB), modal-toast-confirm-surface (each `/layer/*`
      route, CSRF enforcement on `/layer/confirm`, toast auto-dismiss marker),
      ui-state-vocabulary (each state component renders with stable CSS class
      and ARIA live region), feedback-widget (age gate, localStorage gate,
      rate limit, success toast emission).
      (Lives at `tests/integration/admin_interaction_surface.rs` — a
      dev-dependency cycle prevents `crates/openpanel-web/tests`.)
- [x] 1.2 Add unit tests for `crates/openpanel-web/src/forms.rs` covering
      `render_field_error` and `render_field_ok` structural identity (same
      CSS class, same ARIA attributes) across the fixture set.
- [x] 1.3 Add unit tests for `crates/openpanel-web/src/layer.rs` covering
      each route handler's success and forbidden paths, and CSRF token
      embedding inside the confirm fragment.
- [x] 1.4 Add unit tests for `crates/openpanel-web/src/ui_states.rs`
      covering `EmptyState`, `LoadingState`, `ErrorState`, `NoResultsState`
      ARIA and class invariants.
- [x] 1.5 Add domain tests for `crates/openpanel-domain/src/feedback.rs`
      covering `Sentiment`, `FeedbackEntry::validate`, and `FeedbackError`
      variants.
- [x] 1.6 Add repository/service tests for
      `crates/openpanel-app/src/feedback.rs` covering submit, count-by-account-
      in-window (rate limit), and `find_recent`.
- [x] 1.7 Confirm `cargo test -p openpanel-domain` builds with no sqlx /
      axum / tokio references (layering invariant).
- [x] 1.8 Add a property test (`proptest`) for the feedback rate limiter
      showing that 6 submissions within 24h on the same account always
      produce exactly one `429`.

## 2. Domain — feedback aggregate

- [x] 2.1 Create `crates/openpanel-domain/src/feedback.rs` with
      `FeedbackEntry`, `Sentiment` enum, `FeedbackRepository` trait, and
      `FeedbackError`. No `sqlx`/`axum`/`tokio` imports.
- [x] 2.2 Re-export the module from `crates/openpanel-domain/src/lib.rs`.

## 3. App — feedback service + migration

- [x] 3.1 Add SQLite migration in `crates/openpanel-app/src/migrations/`
      creating the `feedback` table (`id`, `account_id`, `sentiment`,
      `comment`, `created_at`) and the index on `(account_id, created_at)`.
- [x] 3.2 Create `crates/openpanel-app/src/feedback.rs` with
      `SqliteFeedbackRepository`, `FeedbackService` (submit, count-in-window,
      find-recent), and `FeedbackModule` (registered via
      `runner.apply_module(FeedbackModule::new(&ctx)...)` like every module).
- [x] 3.3 Wire the module into the CLI and test-support composition roots
      and pass the service to the web router.

## 4. Web — forms.rs (inline validation primitive)

- [x] 4.1 Create `crates/openpanel-web/src/forms.rs` with
      `render_field_error`, `render_field_ok`, and the `422` JSON helper
      (`validation_error_response`).
- [x] 4.2 Re-export from `crates/openpanel-web/src/lib.rs`.
- [x] 4.3 Add `POST /forms/validate` route handler that consumes a single
      field via HTMX and returns the OOB fragment.

## 5. Web — layer.rs (overlay surface)

- [x] 5.1 Create `crates/openpanel-web/src/layer.rs` with
      `GET /layer/{modal,confirm,toast,tip,load}` handlers, each returning
      a typed maud fragment.
- [x] 5.2 Wire the layer router into the web router in
      `crates/openpanel-web/src/router.rs`.
- [x] 5.3 Every destructive action that exists in `sites.rs`, `databases.rs`,
      `cron.rs`, `ssl.rs`, `files.rs`, `users.rs`, `dns.rs`, `ftp.rs` now
      routes through `/layer/confirm`. (`backups.rs` and `mail.rs` expose no
      destructive web action to migrate; `users.rs` has no disable-2fa /
      revoke-token web endpoint; `dns.rs` has no delete-zone web endpoint.)

## 6. Web — ui_states.rs (empty/loading/error/no-results)

- [x] 6.1 Create `crates/openpanel-web/src/ui_states.rs` with the four
      reusable maud components and stable CSS classes
      (`op-empty-state`, `op-no-results`, `op-loading-state`,
      `op-error-state`).
- [x] 6.2 Add `crates/openpanel-web/src/loading.rs` with the
      `htmx-indicator` helper.

## 7. Web — feedback.rs (widget)

- [x] 7.1 Create `crates/openpanel-web/src/feedback.rs` with the
      `POST /feedback` handler, the maud fragment for the widget, and the
      `account_age_days` computation.
- [x] 7.2 Wire the handler into the web router.

## 8. Web — layout.rs (mount layer + feedback widget)

- [x] 8.1 Mount `<div id="layer-root" aria-live="polite">` under the content
      region in `crates/openpanel-web/src/layout.rs`.
- [x] 8.2 Mount `<div id="feedback-widget-root" data-account-age-days="{n}">`
      in `crates/openpanel-web/src/layout.rs`, embedding the widget
      `<template>` only when the server-side age gate passes.
- [x] 8.3 Add the `htmx:afterRequest` listener for `HX-Trigger: layer-toast`
      (inline `<script>` next to the HTMX `<script>` tag, namespaced).

## 9. CSS additions

- [x] 9.1 Add `.op-{toast,modal,confirm,tip,load,empty-state,no-results,
      loading-state,error-state,feedback-widget}` classes to
      `crates/openpanel-web/assets/app.css`, sourcing every colour/spacing
      from `tokens.css`.
- [x] 9.2 Confirm no literal colour values were introduced outside
      `tokens.css` (run the existing literal-color lint).

## 10. Migrate destructive actions

- [x] 10.1 `crates/openpanel-web/src/sites.rs` — delete-site row action
      routes through `/layer/confirm?action=delete-site&id={id}`.
- [x] 10.2 `crates/openpanel-web/src/databases.rs` — delete + password-reveal
      through `/layer/confirm`.
- [x] 10.3 `crates/openpanel-web/src/cron.rs` — delete-job.
- [x] 10.4 `crates/openpanel-web/src/backups.rs` — no destructive web action
      exists (the web page only lists plans/runs and creates plans), so
      there is nothing to migrate; the page now renders UI states instead.
- [x] 10.5 `crates/openpanel-web/src/ssl.rs` — revoke routes through
      `/layer/confirm` (revoke removes the cert).
- [x] 10.6 `crates/openpanel-web/src/files.rs` — delete-entry.
- [x] 10.7 `crates/openpanel-web/src/users.rs` — delete-user routes through
      `/layer/confirm`; no disable-2fa / revoke-token web endpoints exist in
      this route module.
- [x] 10.8 `crates/openpanel-web/src/dns.rs` — delete-record routes through
      `/layer/confirm`; no delete-zone web endpoint exists.
- [x] 10.9 `crates/openpanel-web/src/ftp.rs` — delete-user.
- [x] 10.10 `crates/openpanel-web/src/mail.rs` — no delete-mailbox web
      endpoint exists in this route module; the page now renders UI states.

## 11. List routes render UI states

- [x] 11.1 Sweep every list route
      (`sites`, `databases`, `cron`, `backups`, `ssl`, `files`, `users`,
      `dns`, `ftp`, `mail`, `api_tokens`, `monitoring`, `logs`,
      `notifications`, plus the dashboard alerts region) and replace
      blank-table / blank-panel renders with the appropriate
      `EmptyState` / `NoResultsState`. (`audit` is a 501 stub; no change.)

## 12. Quality gate

- [x] 12.1 `make check` (fmt, clippy, docs, audit, reuse, layering,
      spec-test-drift, literal-scan, tests) — green on 2026-08-18.
- [x] 12.2 `openspec validate add-admin-interaction-surface --strict` — valid.
- [x] 12.3 Update `docs/` with the new layer/forms/feedback usage if any
      developer docs reference form authoring. (None do — `docs/` only
      contains `firewall-recovery.md` and `software-center.md`.)

## 13. Archive

- [ ] 13.1 Run `openspec archive add-admin-interaction-surface`.
