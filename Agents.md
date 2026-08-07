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

**Status:** v0.1-alpha. Identity + auth, sites, databases, and files
ship. SSL, monitoring, cron are planned.

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

**Production-code policy (enforced by `scripts/check-quality.sh`):**

- `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` are **forbidden
  in production code** (`#[cfg(test)]` exempt). Use `?` propagation,
  `match`, or `.expect("invariant: ...")` with a justification.
- `unsafe_code = "forbid"` in production crates.
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

**Workflow gate:** every PR must exit 0 from `./scripts/check-quality.sh`.
Local invocation runs the same steps as CI:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo doc --workspace --no-deps`
4. (optional) `cargo audit`

`add-quality-engineering-infrastructure` defines the full policy.

---

## 6. Configuration Layering

Configuration loads from the lowest-numbered source to the highest:

1. Built-in defaults compiled into the binary
2. `/etc/openpanel/openpanel.toml`
3. `$OPENPANEL_CONFIG` or `./openpanel.toml`
4. `OPENPANEL__*` environment variables (`__` separates path levels)
5. CLI flags

The merged config is validated against a JSON Schema at startup. Bad
config aborts with exit code 78 (`EX_CONFIG`) and a path message.

---

## 7. Agent Workflow Checklist

When asked to implement a feature or spec:

1. **Read** the OpenSpec change folder: `proposal.md`, the cap's
   `specs/<cap>/spec.md`, `design.md`, `tasks.md`. These are
   authoritative.
2. **Check** the source-of-truth specs in `openspec/specs/` for any
   relevant capability (the change's spec is a *delta* on top of
   the source of truth).
3. **Plan** the work by walking `tasks.md` top-to-bottom. The
   `## 1. Testing` group comes first; do not skip ahead.
4. **Implement** in layer order: domain → app → api/cli. Add the
   tests first; ensure they fail; then add the production code; ensure
   they pass.
5. **Smoke-test** at the HTTP layer (`curl` against the local
   server) before declaring done. End-to-end CLI tests live in
   `tests/cli/`.
6. **Run** `scripts/check-quality.sh` locally. Fix every clippy
   warning. Fix every fmt diff.
7. **Update** the change's `tasks.md` — every box checked.
8. **Archive** via `openspec archive <name>`. The delta is folded
   into `openspec/specs/<cap>/spec.md`.
9. **Commit** with a message that follows the existing convention
   (short title on the first line, blank line, detailed body
   explaining *why* and *what*, not just *what*).

---

## 8. Anti-Patterns (do not do these)

- **Don't** write a test that only verifies the current code
  produces the current output. That test will pass when the code is
  wrong.
- **Don't** assume a database is empty or contains specific rows in
  a test. Seed what you need, tear down after.
- **Don't** put `unwrap()` in production code. Use `?`.
- **Don't** mix layers. Domain code MUST NOT import `sqlx`. App code
  MAY import domain but not api. API MAY import app and domain.
- **Don't** edit files outside your bounded context unless the
  composition root needs a one-line change. If you're touching
  identity code from the sites module, you're doing it wrong.
- **Don't** skip the testing group in a tasks.md. Code review MUST
  reject this.

---

## 9. References

- `openspec/specs/architecture/spec.md` — DDD layering contract
- `openspec/specs/identity/spec.md` — auth model
- `openspec/specs/sites/spec.md` — vhost provisioning
- `openspec/specs/databases/spec.md` — MySQL provisioning
- `openspec/specs/files/spec.md` — chrooted file manager
- `openspec/specs/testing/spec.md` — TDD infrastructure (TBD)
- `openspec/specs/quality/spec.md` — quality engineering (TBD)
- `openspec/changes/archive/` — frozen history of every shipped change
- `crates/openpanel-test-support/README.md` — test helpers API
- `tests/README.md` — how to run each test category
- `scripts/check-quality.sh` — single-entry CI script
- `scripts/check-tests.sh` — test gate
- `scripts/coverage.sh` — coverage report (informational)
- `.github/workflows/ci.yml` — CI pipeline
- `clippy.toml` + `rustfmt.toml` — quality policy files