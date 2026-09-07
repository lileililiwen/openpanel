# Agents.md

> This document is the contract for AI agents (and humans) working on
> the OpenPanel codebase. It is **normative**: every principle here
> MUST be followed unless explicitly overridden by a written decision
> in an OpenSpec change.

---

## 1. What is OpenPanel?

OpenPanel is a memory-safe, Rust-based, open-source server
management panel — a Baota / cPanel alternative without PHP. It is a
single static binary with no script engine at runtime. MIT licensed.

**Status:** v0.x in active development. Identity + auth, sites,
databases, files, SSL, mail, DNS, monitoring, cron, software center,
owner-only audit activity center, an mTLS agent fleet, and a pure-Rust
HTMX web UI ship. OpenSpec has 83 live capabilities and 124 archived
changes. The `make check` quality gate is green end-to-end on `main`;
the active change folder `openspec/changes/2026-09-04-restore-make-check-green/`
is awaiting archive + principal ratification of `design.md`. See
`HANDOFF.md` for the current roadmap.

---

## 2. Architecture — DDD, Four Layers

OpenPanel is organised as a Cargo workspace with strict DDD layering.
**Inner layers MUST NOT depend on outer layers.** This is enforced by
separate crates — `cargo` refuses to build if `openpanel-domain`
accidentally imports `sqlx`.

```
        ┌──────────────────────────────────────────────────────────┐
        │  openpanel-cli / openpanel-agent (binaries)              │
        │   HTTP  ◀──▶  axum + tower middleware                    │
        └────────────────────┬─────────────────────────────────────┘
                             │
        ┌────────────────────▼─────────────────────────────────────┐
        │  openpanel-api        (HTTP adapter)                     │
        │  openpanel-cli        (CLI adapter)                      │
        └────────────────────┬─────────────────────────────────────┘
                             │
        ┌────────────────────▼─────────────────────────────────────┐
        │  openpanel-app        (use cases + adapters)            │
        │   IdentityService, SitesService, DatabasesService,      │
        │   FilesService, SQLite repositories                     │
        └────────────────────┬─────────────────────────────────────┘
                             │
        ┌────────────────────▼─────────────────────────────────────┐
        │  openpanel-domain     (entities, VOs, traits)           │
        │   User, Session, Site, Database, Path, FileInfo        │
        │   ZERO I/O — no sqlx, no axum, no tokio                 │
        └─────────────────────────────────────────────────────────┬┘
                             ▲                                     │
        ┌────────────────────┴─────────────────────────────────────┐
        │  openpanel-core        (cross-cutting primitives)        │
        │   Module trait, Config, DatabaseDriver, AuditService,  │
        │   JobSupervisor, tracing, OpenSpec workflow             │
        └──────────────────────────────────────────────────────────┘
```

**Adding a new bounded context** (e.g. monitoring, cron, plugins) is
a 4-step mechanical pattern:

1. `crates/openpanel-domain/src/<bounded>/` — aggregate, value
   objects, error, repository **trait**.
2. `crates/openpanel-app/src/<bounded>/` — use-case service,
   repository impl, MySQL/CLI adapters, module.
3. `crates/openpanel-api/src/routes/<bounded>.rs` — REST handlers,
   DTOs, error mapping.
4. `crates/openpanel-cli/src/handlers.rs` — CLI dispatch.
   Composition root registers the new module via `ModuleRegistry`.

Every bounded context is the same shape — that is the point.

### Module trait contract

Every module implements `openpanel_core::Module`:

```rust
pub trait Module: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn config_schema(&self) -> serde_json::Value { json!({}) }
    fn migrations(&self) -> Vec<Migration> { vec![] }
    fn routes(&self) -> Vec<RouteMount> { vec![] }
    fn background_tasks(&self, _ctx: &AppContext) -> Vec<Box<dyn BackgroundTask>> { vec![] }
}
```

Adding a module = one `register()` call. No editing of unrelated files.

---

## 3. Spec-First Development

Every change to OpenPanel goes through OpenSpec **before** code is
written. The workflow:

```
propose  →  validate  →  implement (apply)  →  archive  →  spec is source of truth
```

### 3.1 OpenSpec workflow

| Phase | What happens | Output |
|---|---|---|
| `propose` | Create `openspec/changes/<name>/` with `proposal.md`, `specs/<cap>/spec.md` (ADDED Requirements), `design.md`, `tasks.md` | A change folder |
| `validate` | Run `openspec validate <name>` | Pass / fail |
| `apply` | Implement tasks in order; mark complete in `tasks.md` | Code |
| `archive` | Run `openspec archive <name>` | Specs folded into `openspec/specs/`; change moved to `archive/` |

### 3.2 Spec lifecycle

```
openspec/changes/<name>/              openspec/specs/<cap>/spec.md
   ├── proposal.md                  ← delta (ADDED Requirements)
   ├── specs/<cap>/spec.md          ← on archive, delta is merged
   ├── design.md                    into the source of truth
   ├── tasks.md                     (shown above)
   └── [during apply]
       ↓ archive
openspec/changes/archive/<date>-<name>/
   └── (frozen copy of the change)
```

**Source of truth** for any capability lives at
`openspec/specs/<cap>/spec.md`. Code that drifts from this is a bug.

### 3.3 Standing rule — tests come first in every change

> **Tests are written before code.** They exist to verify correctness,
> not to accommodate the code so it passes checks. Tests act as
> overseers of the code, not its allies.

Every `tasks.md` for a future code-related change MUST start with
`## 1. Testing` (or its equivalent for docs-only changes). The
testing group MUST list at minimum:

- One unit test per new public function
- One integration test per new HTTP route
- One property-based test per new domain invariant
- One E2E test per new CLI subcommand (if applicable)

The testing group MUST be implemented (and the tests MUST be in a
red/failing state — TDD red phase) **before** any `## 2. Implementation`
tasks are marked complete. Tests MUST be detailed and actionable:
specific inputs, specific assertions, specific edge cases and error
conditions. Vague tests like "test login works" are rejected at code
review.

The `add-tdd-infrastructure` and `add-quality-engineering-infrastructure`
changes define the conventions for what "good" tests look like.

---

## 4. Test Discipline (TDD Infrastructure)

Four test categories with fixed locations:

| Category | Location | Naming |
|---|---|---|
| Unit | `crates/<crate>/src/**/*.rs` `#[cfg(test)] mod tests` | `test_<unit_under_test>` |
| Integration | `tests/integration/<area>.rs` | `<area>_<behavior>` |
| Property | `mod prop` in the same file as the domain code | `prop_<invariant>` |
| E2E (CLI) | `tests/cli/<command>.rs` | `cli_<command>_<scenario>` |

**Repeatability:** tests MUST NOT depend on:

- A running MySQL/Postgres daemon (or the test is skipped, not failed)
- The current contents of any SQLite file (every test gets its own
  `TestDb`)
- Filesystem state created by a previous test
- Environment variables set outside the test

`cargo test --workspace` MUST be order-independent and run twice in a
row with identical results.

`openpanel-test-support` provides `TestDb`, `TestServer`,
`MockUserRepository`, `MockSiteRepository`, `MockDatabaseRepository`,
`MockFileRepository`, `MockAudit`. Test code uses `mockall` for port
mocks — never the concrete SQLite repositories. SQLite is exercised
only via integration tests.

Domain invariants have `proptest` round-trips (≥100 cases by default;
≥1000 in CI). Examples: `Site::new` rejects every `..`; `Password::hash`
is bijective with `verify`; `Path::new` rejects absolute and null bytes.

See `openspec/specs/testing/spec.md` for the full standard.

---

## 5. Quality Engineering

> Use open-source tools to implement checks that prevent runtime
> panics caused by issues like `unwrap`. Tests must be repeatable,
> and you must not make assumptions about the state of the database
> environment.

**Production-code policy (enforced by `make check`):**

- `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` are **forbidden
  in production code** (`#[cfg(test)]` exempt). Use `?` propagation,
  `match`, or `.expect("invariant: ...")` with a justification.
- `unsafe_code = "forbid"` in production crates.
- `#[allow(dead_code)]` is **banned** — dead code is real debt. Restructure
  the code so everything is genuinely used (e.g. one shared test module
  compiled per test binary), or delete the dead path. Never silence the
  lint.
- `cargo fmt --check` on every commit.
- `cargo audit` blocks the build on known RUSTSEC advisories.
- `cargo doc` enforces `rustdoc::broken_intra_doc_links`.

**How to fix violations:**

| Lint hit | Fix |
|---|---|
| `unwrap_used` | Replace with `?`, `match`, or `.expect("invariant: ...")` |
| `expect_used` (with a vague message) | Add an invariant description: `.expect("user_id always set after create_user")` |
| `panic_used` | Replace with a typed error in the function's return |
| `todo` / `unimplemented` | Either implement it now or remove the code path |
| `too_many_arguments` | Either group into an options struct or `#[allow]` with a justification comment |

**Why:** a passing test does not guarantee safe code. A test that
calls `.unwrap()` on input it never received will still panic in
production. Static analysis catches what tests don't.

**Workflow gate:** every PR must exit 0 from `make check`.
Local invocation runs the same gates as CI, in this order:

1. `make fmt` — `cargo fmt --all -- --check`
2. `make clippy` — `cargo clippy --workspace --all-targets -- -D warnings`
3. `make docs` — `cargo doc --workspace --no-deps`
4. `make audit` — `cargo audit` (optional, skipped if tool absent)
5. `make file-length` — per-file line-count lint (optional)
6. `make scan-literal` — literal colour / string scan
7. `make class-coverage` — every `class="..."` literal has a rule in `app.css`
8. `make tasks-testing-first` — `## 1. Testing` comes first in `tasks.md`
9. `make reuse` — no duplicated public item across crates
10. `make layering` — domain MUST NOT import app/api
11. `make spec-test-drift` — every spec has a covering test
12. `make spec-drift` — every archived delta exists in the live spec
13. `make agent-governance` — OpenSpec context + runtime contract integrity
14. `make governance-contract` — archived governance content ratchet
15. `make test-gates` — the governance gates' own positive/negative fixtures
16. `make test` — full test suite

`add-quality-engineering-infrastructure` defines the full policy.

### 5.1 Archived governance ratchet

`make spec-drift` only proves that an archived delta's requirement
*heading* still exists in the live spec. It does not prove the text, the
scenarios, or the executable protection survived — and governance is
exactly what gets weakened while ordinary product tests stay green.

`make governance-contract` (`scripts/check-governance-contract.sh`) is
the focused ratchet for the four governance capabilities
(`agent-quality`, `quality`, `testing`, `architecture`):

- **Manifest** — `openspec/governance/manifest.yaml` pins each protected
  requirement by archive path, capability, requirement name, content
  digest (normalized) and scenario count.
- **Executable protection** — every entry names at least one checker ID,
  and that ID MUST be registered in `scripts/test-gates.sh` with BOTH a
  positive and a negative fixture (`# checker: <id> positive` /
  `# checker: <id> negative`). A checker with only a happy-path fixture
  is treated as orphaned, i.e. unprotected.
- **Disposition** — every archived governance requirement is either
  protected (manifest entry) or recorded as tracked debt in
  `openspec/governance/unprotected-baseline.txt`. A newly archived
  governance requirement with neither fails the build until a human
  reviews it. The baseline is debt, not a waiver: it only shrinks.
- **Read-only** — the gate never rewrites the manifest, the baseline or
  any spec, and its diagnostics name the archive/capability/requirement
  without printing requirement contents.

**How to update a protected requirement (reviewed procedure):**

1. Propose an OpenSpec change whose delta edits the live requirement.
2. Run `scripts/check-governance-contract.sh --report`.
3. In the same reviewed change, update that entry's `digest` and
   `scenarios` from the report. `make check` is red until you do.
4. To give an unprotected requirement real protection, add positive +
   negative fixtures for it in `scripts/test-gates.sh`, add the manifest
   entry, then regenerate the baseline with
   `scripts/check-governance-contract.sh --write-baseline`.

### 5.2 Per-change commit cadence (commit 1, then commit 2)

A change produces **two commits** in this order:

- **Commit 1 — the change.** `openspec archive <name> --yes` moves
  the spec deltas into the live specs and ratchets the manifest.
  `git add` and commit (a) the implementation files (Rust / CSS /
  shell / test), (b) the new gate scripts / Makefile / new
  assets, (c) the live-spec updates (the now-merged delta under
  `openspec/specs/<cap>/spec.md` and the manifest entry), (d) the
  ticked `tasks.md`, and (e) the now-archived change folder under
  `openspec/changes/archive/<name>/`. Use a `feat(...)` / `chore(...)`
  / `fix(...)` prefix that matches the change's nature. When the
  live spec + manifest updates have been pre-applied (e.g. an
  earlier turn of the agent), pass `--skip-specs` to
  `openspec archive` so it does not re-apply the already-merged
  delta.
- **Commit 2 — the HANDOFF follow-up.** Update `HANDOFF.md` to
  reflect the new status (the change's row in the progress table
  becomes `[x] implemented; archived & committed (<hash>)` using
  the commit hash from commit 1; the "Next steps" entry moves to
  the next change). `git add` and commit `HANDOFF.md` only. The
  same change may also touch `AGENTS.md` / `README.md` to add a
  new gate, a new example, or a quick-reference tweak; those
  edits are part of commit 2.

Do NOT amend commit 1 to add the HANDOFF update; do NOT commit
them together. The two-commit split keeps commit 1 a clean change
commit (revertable, bisectable, the unit of "what this change
did") and commit 2 the post-hoc bookkeeping that depends on
commit 1's hash.

---

## 6. Security — private-key & secret handling

Secrets (passwords, session tokens, TLS private keys) MUST NOT leak.
The same rules apply to code, logs, error messages, DB rows, API
responses, and CLI output:

- **Never log secrets.** No `info!`/`debug!`/`error!` line may print a
  plaintext password, session token, or PEM private key — even at
  debug level.
- **Never return secret material.** TLS private keys (plaintext or
  ciphertext) never appear in any API/CLI response; list/get replies
  are metadata-only.
- **Encrypt at rest.** Passwords and TLS private keys live in the DB
  as AES-256-GCM ciphertext under the master key
  (`OPENPANEL__DATABASE__MASTER_KEY`). The ciphertext layout is
  `hex(nonce) || ":" || hex(ciphertext)` (see `crypto.rs`).
- **The plaintext key only touches disk** in `SslPaths::key_dir`
  (mode `0600`) for nginx to read; never in the DB, never in an
  error, never in a test assertion unless the test asserts ciphertext
  (not valid PEM).
- **Challenge server is local-only.** The ACME HTTP-01 challenge
  server binds to `127.0.0.1:9080`; nginx proxies
  `/.well-known/acme-challenge/` to it. It MUST NOT bind to a public
  interface.
- **Staging default.** ACME issuance defaults to Let's Encrypt
  **staging**; production requires an explicit opt-in flag. Tests
  never hit the real production endpoint.

---

## 7. Configuration Layering

Configuration loads from the lowest-numbered source to the highest:

1. Built-in defaults compiled into the binary
2. `/etc/openpanel/openpanel.toml`
3. `$OPENPANEL_CONFIG` or `./openpanel.toml`
4. `OPENPANEL__*` environment variables (`__` separates path levels)
5. CLI flags

The merged config is validated against a JSON Schema at startup. Bad
config aborts with exit code 78 (`EX_CONFIG`) and a path message.

---

## 8. Agent Workflow Checklist

When asked to implement a feature or spec:

1. **Read** the OpenSpec change folder: `proposal.md`, the cap's
   `specs/<cap>/spec.md`, `design.md`, `tasks.md`. These are
   authoritative. Also load `openspec/specs/agent-quality/spec.md` —
   it is the top-level contract for how you must work.
2. **Check** the source-of-truth specs in `openspec/specs/` for any
   relevant capability (the change's spec is a *delta* on top of
   the source of truth).
3. **Explore & Reuse** *(mandatory, before planning or coding)* —
   run `scripts/repo-map.sh` and grep the tree for existing
   utilities, traits, repository implementations, and modules that
   already satisfy the requirement. In `design.md`, **name the exact
   existing code you will reuse** and justify any new code. Do not
   re-implement something that already exists (the `reuse` gate in
   `make check` will reject duplicates).
4. **Plan** the work by walking `tasks.md` top-to-bottom. The
   `## 1. Testing` group comes first; do not skip ahead.
5. **Compaction check** — keep your working context focused. After the
   plan phase, write a one-paragraph status (goal, approach, done,
   current blocker) into the change folder so the context window does
   not fill with noise that causes mid-task hallucination.
6. **Implement** in layer order: domain → app → api/cli. Add the
   tests first; ensure they fail; then add the production code; ensure
   they pass.
7. **Smoke-test** at the HTTP layer (`curl` against the local
   server) before declaring done. End-to-end CLI tests live in
   `tests/cli/`.
8. **Run** `make check` locally. Fix every clippy
   warning. Fix every fmt diff.
9. **Update** the change's `tasks.md` — every box checked.
10. **Human review gate** — a change MUST NOT be implemented (`apply`)
    until its `design.md` has been reviewed and approved by a human
    principal. Reviewing the research and plan catches architectural
    drift and hallucinated assumptions *before* they reach the tree.
11. **Archive** via `openspec archive <name>`. The delta is folded
    into `openspec/specs/<cap>/spec.md`. If the change touches one of
    the four governance capabilities (`agent-quality`, `quality`,
    `testing`, `architecture`), the archive step makes
    `make governance-contract` red until you give the new requirement a
    reviewed disposition — a manifest entry with a checker, or a
    baseline entry (see §5.1).
12. **Commit** with a message that follows the existing convention
    (short title on the first line, blank line, detailed body
    explaining *why* and *what*, not just *what*).

> **Global-view rule:** if you cannot point at the existing module or
> utility your change depends on, stop and explore (`scripts/repo-map.sh`)
> before writing code. Confidently inventing an API that already exists
> elsewhere is the most expensive failure mode.

---

## 9. Anti-Patterns (do not do these)

- **Don't** write a test that only verifies the current code
  produces the current output. That test will pass when the code is
  wrong.
- **Don't** assume a database is empty or contains specific rows in
  a test. Seed what you need, tear down after.
- **Don't** put `unwrap()` in production code. Use `?`.
- **Don't** add `#[allow(dead_code)]` to silence a warning. Either use the
  code or delete it; if a shared test helper has fields unused in some
  test binaries, consolidate them into a single test target.
- **Don't** mix layers. Domain code MUST NOT import `sqlx`. App code
  MAY import domain but not api. API MAY import app and domain.
- **Don't** edit files outside your bounded context unless the
  composition root needs a one-line change. If you're touching
  identity code from the sites module, you're doing it wrong.
- **Don't** skip the testing group in a tasks.md. Code review MUST
  reject this.

---

## 11. Web UI Styling & Responsive Layout (anti-regression)

> **Every web UI route ships styled AND responsive.** Browser-default
> `<input>` / `<textarea>` / `<button>` styling and fixed-width
> desktop layouts that break on a phone are a regression and MUST
> NOT ship.

### 11.1 Form styling is a global contract, not per-form opt-in

The legacy ad-hoc `.form` / `.login` classes are **forbidden for new
code**. Every `<form>` MUST use the global `tokens.css` design
language and the shared `form`, `form-row`, `form-actions`, and
`form-grid` classes declared in the web-ui styling spec (see
`openspec/specs/web-ui-styling/spec.md`).

Concretely, a new `<form>` MUST:

- Be wrapped in `<form class="form" novalidate>` (or `form form-row`,
  `form form-grid`) so its descendants inherit the token vocabulary.
- Source colours, spacing, radii, and typography **only** from the
  custom properties in `tokens.css`. Hard-coded hex codes,
  pixel-spacing, or non-token fonts are rejected at code review.
- Pair every `<input>`, `<textarea>`, `<select>` with a sibling
  `<label>` (a11y baseline) and a typed `:focus-visible` ring
  (`outline: 2px solid var(--op-color-focus-ring)`).
- Use `<button type="submit">` for primary actions and
  `<button type="button">` for in-page toggles; never bare
  `<button>` without a type.
- Show validation errors via `<p class="form-error" role="alert">`
  (announced by screen readers).

### 11.2 Responsive layout is mandatory at three breakpoints

Every page MUST be tested at three widths before merging:

| Breakpoint | Target | Layout |
|---|---|---|
| ≥ 1024 px | desktop | full sidebar + content |
| 640 px – 1023 px | tablet | collapsible sidebar; content fits without horizontal scroll |
| < 640 px | mobile | stacked top bar; full-width forms; tables switch to card layout or scroll horizontally inside the card |

CSS uses the **mobile-first** pattern:

```css
/* Default = mobile */
.card { padding: var(--op-space-3); }

@media (min-width: 640px) {
  /* tablet */
  .card { padding: var(--op-space-4); }
}

@media (min-width: 1024px) {
  /* desktop */
  .layout { grid-template-columns: 240px 1fr; }
}
```

`max-width: <number>` queries are forbidden — they cascade
incorrectly and break on viewports in between.

### 11.3 Accessibility baseline (do not regress)

- Every interactive element has accessible text (`aria-label` or
  visible label).
- Focus is always visible (`:focus-visible` ring is mandatory).
- All text meets WCAG 2.1 AA contrast against `tokens.css` colours;
  the follow-on `refine-quality-with-i18n-and-theme-policy` change
  ships the contrast test that enforces this.
- Every page has a single `<h1>`; heading levels never skip.

### 11.4 How to add a new form (checklist)

1. Pick the smallest token vocabulary that fits the form
   (`form`, `form form-row`, or `form form-grid`).
2. Render the form with `<form novalidate>` (the browser's native
   validation is replaced by `tokens.css` styling).
3. Pair every input with a `<label>`.
4. Add a `@media (min-width: 640px)` block only if the mobile layout
   needs tablet-specific spacing.
5. Smoke-test at 360 px (iPhone), 768 px (iPad portrait), and
   1280 px (laptop). Capture a screenshot at each width.
6. Run `scripts/scan-template-literals.sh` (the literal-string
   scan). It MUST pass.
7. Re-run `make check`. New clippy lints for non-token literals in
   `app.css` will reject the change.

### 11.5 What NOT to do

- **Don't** ship a `<form>` without a class. The default styling
  IS the regression; this is what we are guarding against.
- **Don't** introduce a one-off form class (`.foo-form`, `.bar-form`).
  Extend the shared `form` vocabulary or open a spec change first.
- **Don't** use `px` in CSS. Use `var(--op-space-*)`.
- **Don't** declare a new colour, font, or radius outside `tokens.css`.
- **Don't** write `@media (max-width: ...)`. Use `min-width`.
- **Don't** hide content with `display: none` to "fix" a responsive
  bug. Restructure the markup.
- **Don't** skip the three-breakpoint smoke test. If you can't run
  it, the change is not ready for review.

### 11.6 CI enforcement

The follow-on `web-ui-styling` change ships:

- A `tests/web_ui_styling.rs` integration suite that loads each
  public web route at 360 / 768 / 1280 px and asserts no horizontal
  overflow at any width.
- A `templates-no-browser-defaults` clippy-style lint that fails
  the build when a `<form>` lacks a class.
- A `tokens-only` lint that fails when CSS outside `tokens.css`
  declares a literal colour, spacing value, or font family.

Until those land, **every PR adding or modifying a web route must
include a screenshot** at the three breakpoints above in the PR
description.

### 11.7 References

- `openspec/specs/web-ui-styling/spec.md` — single source of truth
  for the styling + responsive contract
- `crates/openpanel-web/assets/tokens.css` — design-token vocabulary
- `crates/openpanel-web/assets/app.css` — current stylesheet
- `crates/openpanel-web/src/layout.rs` — shell / sidebar / topbar
- `crates/openpanel-web/src/login.rs` — canonical styled-form example
  (login form), referenced as the pattern to follow

---

- `openspec/specs/architecture/spec.md` — DDD layering contract
- `openspec/specs/identity/spec.md` — auth model
- `openspec/specs/sites/spec.md` — vhost provisioning
- `openspec/specs/databases/spec.md` — MySQL provisioning
- `openspec/specs/files/spec.md` — chrooted file manager
- `openspec/specs/ssl/spec.md` — TLS certificate lifecycle
- `openspec/specs/monitoring/spec.md` — host resource monitoring
- `openspec/specs/mail/spec.md` — hosted mail domains, mailboxes, and safe MTA/IMAP configuration
- `openspec/changes/add-web-ui-foundation/specs/web-ui/spec.md` — web UI (login, shell, CSRF)
- `openspec/specs/testing/spec.md` — TDD infrastructure (TBD)
- `openspec/specs/quality/spec.md` — quality engineering (TBD)
- `openspec/changes/archive/` — frozen history of every shipped change
- `crates/openpanel-test-support/README.md` — test helpers API
- `crates/openpanel-app/src/ssl/README.md` — SSL module internals
- `crates/openpanel-app/src/monitoring/README.md` — monitoring module internals
- `crates/openpanel-web/` — pure-Rust HTMX web UI (`layout`, `login`, `csrf`, `assets`, `router`)
- `tests/README.md` — how to run each test category
- `Makefile` — single-entry quality gate (`make check`)
- `scripts/check-fmt.sh`, `scripts/check-clippy.sh`,
  `scripts/check-docs.sh`, `scripts/check-audit.sh` — per-gate scripts
- `scripts/check-tests.sh` — test gate
- `scripts/coverage.sh` — coverage report (informational)
- `.github/workflows/ci.yml` — CI pipeline
- `clippy.toml` + `rustfmt.toml` — quality policy files

---

## 12. References

- `openspec/specs/architecture/spec.md` — DDD layering contract
- `openspec/specs/identity/spec.md` — auth model
- `openspec/specs/sites/spec.md` — vhost provisioning
- `openspec/specs/databases/spec.md` — MySQL provisioning
- `openspec/specs/files/spec.md` — chrooted file manager
- `openspec/specs/ssl/spec.md` — TLS certificate lifecycle
- `openspec/specs/monitoring/spec.md` — host resource monitoring
- `openspec/specs/mail/spec.md` — hosted mail domains, mailboxes, and safe MTA/IMAP configuration
- `openspec/specs/web-ui-styling/spec.md` — form styling + responsive contract
- `openspec/specs/web-ui/spec.md` — web UI (login, shell, CSRF)
- `openspec/specs/testing/spec.md` — TDD infrastructure
- `openspec/specs/quality/spec.md` — quality engineering
- `openspec/specs/agent-quality/spec.md` — agent contract (load first)
- `openspec/governance/manifest.yaml` — ratchet for the four
  governance capabilities (`agent-quality`, `quality`, `testing`,
  `architecture`)
- `openspec/governance/unprotected-baseline.txt` — tracked debt
  (only shrinks)
- `openspec/changes/archive/` — frozen history of every shipped change
- `HANDOFF.md` — live roadmap + spec status (current change table)
- `crates/openpanel-test-support/README.md` — test helpers API
- `crates/openpanel-app/src/ssl/README.md` — SSL module internals
- `crates/openpanel-app/src/monitoring/README.md` — monitoring module internals
- `crates/openpanel-web/` — pure-Rust HTMX web UI (`layout`, `login`,
  `csrf`, `assets`, `router`, `dashboard`, `site_workspace`, `audit`,
  `software_center`, `ops_workflows`, `host_fleet`,
  `software_center_trust`)
- `tests/README.md` — how to run each test category
- `Makefile` — single-entry quality gate (`make check`)
- `scripts/check-fmt.sh`, `scripts/check-clippy.sh`,
  `scripts/check-docs.sh`, `scripts/check-audit.sh` — per-gate scripts
- `scripts/check-tests.sh` — test gate
- `scripts/check-file-length.sh` — per-file line-count lint
- `scripts/check-tasks-testing-first.sh` — `## 1. Testing` first
- `scripts/check-reuse.sh` — no duplicated public item across crates
- `scripts/check-layering.sh` — domain MUST NOT import app/api
- `scripts/check-spec-test-drift.sh` — every spec has a covering test
- `scripts/check-spec-drift.sh` — every archived delta exists in the
  live spec
- `scripts/check-agent-governance.sh` — OpenSpec context + runtime
  contract integrity (drives `make agent-governance`)
- `scripts/check-governance-contract.sh` — archived governance
  content ratchet (drives `make governance-contract`)
- `scripts/test-gates.sh` — governance self-test harness (16
  fixture-based checks; drives `make test-gates`)
- `scripts/scan-template-literals.sh` — literal colour / string scan
- `scripts/coverage.sh` — coverage report (informational)
- `scripts/repo-map.sh` — structural map of public APIs (agent aid)
- `.github/workflows/ci.yml` — CI pipeline (runs `make check` in the
  `check` job; `make test-gates` + strict OpenSpec validation in
  `agent-quality`)
- `clippy.toml` + `rustfmt.toml` — quality policy files
- `cargo-lint-extra.toml` — file-length thresholds (soft_limit, hard_limit)
