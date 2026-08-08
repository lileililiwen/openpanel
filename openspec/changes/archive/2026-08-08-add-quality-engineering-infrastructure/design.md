# Design: Add Quality Engineering Infrastructure

## Context

The TDD change (`add-tdd-infrastructure`) gives us a test layer that
catches behavioural regressions. But a passing test does not
guarantee safe production code. Code that calls `.unwrap()` on
input it never received in tests will still panic in production.

This change adds the static-analysis + policy layer on top of TDD.
It operationalises the rule from `Agents.md`:

> Use open-source tools to implement checks that prevent runtime
> panics caused by issues like `unwrap`.

## Goals / Non-Goals

**Goals:**

- Clippy deny for unwrap/expect/panic in non-test code, with `?`
  propagation required.
- `cargo fmt --check` enforced in CI.
- `cargo audit` for known vulnerabilities in the dep graph.
- `rustdoc::broken_intra_doc_links` enforced.
- `unsafe_code = "forbid"` in production crates.
- A single `make check` entry point that dispatches per-gate scripts.
- Informational GitHub Actions workflow.
- Informational coverage report.

**Non-Goals:**

- Coverage thresholds (deferred; coverage report is informational).
- Mutation testing (deferred; `cargo-mutants` not in offline cache).
- Strict `cargo-deny` (e.g. license whitelist, multiple-versions
  check) — `cargo-deny` not in offline cache; can be added later.
- Custom clippy lints.
- Auto-fixers (e.g. `cargo fix` for the lint policy).
- A real CI server integration (informational workflow only).

## Decisions

### 1. Workspace-level `[lints.rust]` table

**Decision**: Set clippy lints at the workspace level via
`[workspace.lints.rust]` in root `Cargo.toml`. Each crate inherits
by default; specific crates can override.

**Rationale**: Centralised config; no per-crate duplication.

### 2. `unsafe_code = "forbid"` workspace-wide

**Decision**: Forbid unsafe at the workspace level. Test code
implicitly allows `unsafe` because `forbid` can be relaxed via
`#[allow(unsafe_code)]` inside `#[cfg(test)]` modules.

**Rationale**: OpenPanel is not a systems-programming project; unsafe
is never required. Forbidding makes the question "is this unsafe
necessary?" impossible to answer with "yes".

### 3. `cargo fmt` with project defaults

**Decision**: Inherit `cargo fmt` defaults. Document the formatter
in `Agents.md` so contributors don't run with odd personal
configs.

**Rationale**: No formatting taste to enforce beyond the standard
tool's defaults; keeps CI diffs minimal.

### 4. `cargo audit` as a hard gate

**Decision**: `cargo audit` failure blocks the build. Suppressions
live in `.cargo/audit.toml` (cargo-audit's default config location)
with a `reason`.

**Rationale**: A known RUSTSEC advisory is a real risk; ignoring it
in CI is the same as ignoring a CVE. The suppression mechanism lets
us acknowledge a false positive with an expiry.

### 5. `make check` dispatches each gate to its own script

**Decision**: A root `Makefile` is the quality gate *manager*. It
declares the gates and their order (`fmt clippy docs audit test`) but
holds no business logic. Each gate is one small script under
`scripts/` (`check-fmt.sh`, `check-clippy.sh`, `check-docs.sh`,
`check-audit.sh`, `check-tests.sh`) that shares a single `step`
helper (`scripts/lib/step.sh`) for the machine-parseable
`step: <name> status: ok | failed` lines.

**Rationale**: One entry point for CI (`make check`), while keeping
each check independently runnable and fixable — a single fat script
that does everything is hard to maintain. Composition over
duplication, dispatch over implementation.

### 6. Coverage is informational in v0.1

**Decision**: `scripts/coverage.sh` produces an lcov report but does
NOT fail the build on low coverage. A future change will introduce
a threshold.

**Rationale**: Current coverage is unknown. Setting a threshold
prematurely would either block the build or pass without meaning.
Informational gives the team time to learn the baseline.

## Risks / Trade-offs

- **Risk**: clippy deny rules surface many existing violations at
  once → *Mitigation*: this change MUST fix all of them; the spec's
  "Existing `unwrap`s ... are eliminated" scenario enforces this.
- **Risk**: `cargo audit` requires network access → *Mitigation*:
  cache the advisory database in CI; if offline, skip with a clear
  warning.
- **Risk**: `unsafe_code = "forbid"` breaks legitimate use in
  dependencies that expose `unsafe` → *Mitigation*: only applies to
  crates we own; deps are unaffected.

## Migration Plan

- Apply clippy lints, fix every `unwrap` / `expect` / `panic` /
  `todo` / `unimplemented` in production code.
- Run `cargo fmt` once across the workspace to normalise formatting.
- Add `clippy.toml` and `[workspace.lints.rust]`.
- Add `scripts/lib/step.sh` and one per-gate script under `scripts/`
  (`check-fmt.sh`, `check-clippy.sh`, `check-docs.sh`,
  `check-audit.sh`); reuse `check-tests.sh` from the TDD change.
- Add the root `Makefile` that dispatches them (`make check`).
- Add `.github/workflows/ci.yml` (informational).
- Update `Agents.md` with the new section.

## Open Questions

- Should `cargo deny` (license, multiple-versions) be added now or
  later? → *Default: later* — not in offline cache.
- Should clippy's nursery group be enabled? → *Default: no* for v0.1
  to keep the diff small; can be added incrementally.