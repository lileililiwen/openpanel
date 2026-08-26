## Context

OpenPanel's web adapter (`crates/openpanel-web/src/`) is a server-rendered
maud + HTMX shell with 40+ feature route files. A UX audit against
aaPanel's verified `BTPanel/static/js/public.js` and `layout.html`, plus
the NN/g form-error heuristics, found that OpenPanel's reported
"interaction feels weird" stems from a structural gap: no centralized
interaction grammar. aaPanel funnels every overlay through one library
(`layui.layer`); OpenPanel's routes each render their own HTMX swap with
no modal/toast/confirm/loading primitive, no inline-validation hooks,
and no feedback rail. The fix is structural, not per-page.

## Goals / Non-Goals

**Goals:**

- A single layer module that owns every overlay (modal, confirm, toast,
  tip, load) — modals reserved for destructive actions only.
- A server contract for inline-near-field validation on blur with `422`
  + JSON error map + HTMX OOB swap.
- A reusable `EmptyState` / `LoadingState` / `ErrorState` / `NoResultsState`
  vocabulary so no list route renders a blank panel.
- A non-nagging feedback widget gated on `account_age_days >= 3` AND
  `localStorage.openpanel_feedback_seen` unset (mirrors aaPanel's
  verified `safe_day + localStorage.NPS` pattern).
- Migrate every existing destructive action to `layer.confirm`.

**Non-Goals:**

- Adopting layui, React, Vue, or any client-side framework.
- Changing the visual theme, design tokens, or navigation structure.
- A full Baota parity pass (Baota-specific claims were unverified).
- Replacing the existing CSRF or session model.

## Decisions

### D1. Layer is a server-rendered fragment surface, not a JS library

**Choice:** `crates/openpanel-web/src/layer.rs` exposes `GET /layer/{modal,confirm,toast,tip,load}` returning HTMX fragments. No new client-side JS library.

**Rationale:** Matches the maud + HTMX constraint (no build step). aaPanel uses `layer.open/msg/confirm/tips` because layui is already vendored; OpenPanel has no equivalent runtime, so the layer must be server-rendered fragments dispatched via HTMX OOB swaps.

**Alternatives considered:**

- *htmx-extensions/headers + custom client JS* — rejected: needs a
  runtime dependency and an extra extension; we already vendored
  `htmx.min.js`.
- *Alpine.js or Stimulus for client-side reactivity* — rejected: violates
  the "no client-side framework" rule.

### D2. Inline validation triggers on `blur`, not `input`

**Choice:** Each field uses `hx-trigger="blur changed"` so the server is
hit only after the user finishes interacting with the field.

**Rationale:** NN/g verified guideline: "Validation must not be triggered
before the user has finished the field." Triggering on every keystroke
both overloads it and surfaces errors before the user has typed a
complete value.

**Alternatives considered:**

- *Debounced `input`* — rejected: still fires mid-typing for short fields.
- *Submit-only validation* — rejected: doesn't meet the inline-near-field
  requirement (errors render at the top of the form or in a modal).

### D3. 422 with JSON `{errors:{field:message}}` is the wire contract

**Choice:** Form handlers return `422 Unprocessable Entity` with a JSON
body matching that shape plus an optional HTMX fragment for OOB swap.

**Rationale:** Gives both HTMX (consumes the fragment) and non-HTMX
clients (consume the JSON) a usable contract. Mirrors REST conventions
for validation failure.

**Alternatives considered:**

- *Always return 200 with embedded error* — rejected: hides errors from
  monitoring and breaks the "top-of-form summary alone is not allowed"
  guideline.
- *Return 400 with no body* — rejected: 400 is ambiguous; 422 is the
  standard RFC 4918 code for validation failure.

### D4. Feedback widget is server-side gated + localStorage gated

**Choice:** The shell renders the gating metadata in
`data-account-age-days`; client JS reads it and either inserts the
widget or leaves the container empty. Submission persists to a new
`feedback` table.

**Rationale:** Server gating alone is too coarse — it would nag every
returning user. localStorage alone would not respect the 3-day safe
threshold for new accounts. Both are needed, exactly as in aaPanel's
verified pattern (`layout.html:242,627-631`).

**Alternatives considered:**

- *Pure server gating via cookie* — rejected: cookies are 1st-party but
  the user can clear them; localStorage is the right scope for
  "have I seen this widget on this browser."
- *Track dismissal server-side only* — rejected: doesn't reduce nagging
  for users with multiple browsers or cleared storage.

### D5. DDD layering: feedback lives in `domain`, app, then web

**Choice:** New `crates/openpanel-domain/src/feedback.rs` defines
`FeedbackEntry`, `Sentiment`, `FeedbackError`, `FeedbackRepository`.
`crates/openpanel-app/src/feedback.rs` holds the SQLite impl and a
`FeedbackService`. The web adapter adds a thin `feedback.rs` route +
maud fragment.

**Rationale:** Matches the strict layering enforced by AGENTS.md:
domain has zero I/O, app orchestrates, web adapter renders. The new
bounded context follows the existing
`domain/<ctx>/` + `app/<ctx>/` + `web/<ctx>.rs` shape.

### D6. UI state components live in `crates/openpanel-web/src/ui_states.rs`

**Choice:** Single new file with four maud components; every list route
calls one of them.

**Rationale:** A single file is the lightest-weight home for a shared
view vocabulary. Co-locating with the web crate (not domain) is correct
because these are presentation concerns.

### D7. CSS additions stay inside the existing token system

**Choice:** Add `.op-{toast,modal,confirm,empty-state,loading-state,
error-state,no-results}` classes in `crates/openpanel-web/assets/app.css`,
sourcing every colour/spacing value from `tokens.css`.

**Rationale:** The existing `web-ui-styling` requirement forbids literal
colour values outside `tokens.css`. Layer CSS must respect that.

## Explore & Reuse

| Reuse target | Where | Why we reuse instead of re-implementing |
|---|---|---|
| `crates/openpanel-web/src/layout.rs` | shell | Mount `#layer-root` and `#feedback-widget-root` inside the existing layout — do not introduce a parallel shell. |
| `crates/openpanel-web/src/nav_model.rs` | navigation | No changes needed; the new layer/feedback widgets are not navigation. |
| `crates/openpanel-web/src/web_ui_styling.rs` | styling helpers | Existing CSS class generators are reused for the new components. |
| `crates/openpanel-web/src/themeable_ui.rs` | theming | Layer/toast/widget colours MUST pull from the existing token system. |
| `crates/openpanel-web/src/csrf.rs` | CSRF | The layer confirm form reuses the existing per-session token. No new CSRF logic. |
| `crates/openpanel-web/assets/htmx.min.js` | HTMX runtime | Existing vendored bundle; we add an inline `htmx:afterRequest` listener only — no new dependency. |
| `crates/openpanel-app/src/` modules | bounded contexts | The new `feedback` module is wired via `app.register(FeedbackModule)` like every other context — do not invent a parallel wiring path. |
| `crates/openpanel-domain/src/` modules | domain types | `Sentiment`, `FeedbackEntry`, `FeedbackError`, `FeedbackRepository` follow the existing pattern (e.g. `crates/openpanel-domain/src/notifications/feedback.rs`-style modules). |
| `crates/openpanel-web/src/{sites,databases,cron,backups,ssl,files,users,dns,ftp,mail}.rs` | destructive actions | Each existing destructive button becomes an `hx-get="/layer/confirm?action=..."` link — no logic duplication. |

## Risks / Trade-offs

- **Risk:** HTMX OOB swap timing on slow networks could let a user submit
  twice before the validation response lands.
  **Mitigation:** Every destructive form has `hx-disabled-elt` on the
  submit button; `htmx:beforeRequest` toggles `aria-busy` and disables
  the button.

- **Risk:** The feedback widget could still nag if `localStorage` is
  cleared per-session.
  **Mitigation:** Acceptable per the verified aaPanel pattern — the
  account-age gate (`>= 3` days) plus server-side rate limit (5 per
  account per 24h) bounds the worst case to one widget per browser
  reset per mature account per day.

- **Risk:** Server-rendered toasts require a round-trip per success
  feedback.
  **Mitigation:** Toasts are tiny fragments and the network cost is
  negligible at panel scale; aaPanel uses the same trade-off.

- **Risk:** Adding `htmx:afterRequest` hook via inline JS risks
  duplicating other listeners.
  **Mitigation:** The hook is namespaced
  (`document.body.addEventListener('htmx:afterRequest', ...)`) and uses
  feature-detection on `evt.detail.xhr.getResponseHeader('HX-Trigger')`.

## Migration Plan

1. Land specs + design (this change) and obtain human approval.
2. Land `crates/openpanel-domain/src/feedback.rs` + migration + tests
   (no UI change yet).
3. Land `crates/openpanel-web/src/{forms,layer,ui_states,loading,feedback}.rs`
   + assets + integration test file. Mount `#layer-root` and
   `#feedback-widget-root` in `layout.rs`.
4. Migrate destructive actions in `sites.rs`, `databases.rs`,
   `cron.rs`, `backups.rs`, `ssl.rs`, `files.rs`, `users.rs`,
   `dns.rs`, `ftp.rs`, `mail.rs` one PR at a time, each behind a
   flag in the existing route module.
5. After every destructive route migrates, remove the temporary flag
   paths.

**Rollback:** Each layer route is a new addition; reverting
`layout.rs` to its pre-change state removes the layer mount, and the
destructive actions revert to their previous (non-confirm) form on a
per-file basis. The migration is per-route, so a partial rollback is
possible without rebuilding the binary.

## Open Questions

- **Q1:** Should the `feedback` aggregate also capture the panel version
  and current route, to help correlate feedback with releases? (Suggested:
  yes — adds two columns; awaits human approval.)
- **Q2:** Should `layer.tip` render via CSS-only `:hover` for non-critical
  help, with HTMX-loaded detail for long descriptions? (Suggested: CSS-only
  first, HTMX-loaded detail only for field documentation links.)
- **Q3:** The audit found Baota-specific patterns were unverified. Should
  we run a Baota-focused second audit before claiming "Baota parity"?
  (Suggested: yes, after this change ships, so we can spot-check the
  gap.)