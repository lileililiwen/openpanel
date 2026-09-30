# Design: Post-MVP documentation readiness — Web UI screenshots and capability-catalog linkage

## Context and Boundary

Repository-local, documentation-only. In scope:

- `README.md` (screenshots subsection + capability-catalog link)
- a new `docs/screenshots/` asset directory
- a generated capability catalog (e.g. `docs/CAPABILITIES.md`) with a small
  generator script
- an extension to `scripts/check-docs.sh` (the `make docs` leg) plus
  `scripts/test-gates.sh` fixtures
- possibly a README `make check` chain correction

Out of scope: `crates/**` runtime code, migrations, schemas, the governance
manifest, and the in-flight `add-portable-host-operations-and-migration` change
and its untracked files.

## Explore & Reuse

Read `scripts/repo-map.sh` output and the existing surfaces before writing:

- **Capability registry** — `crates/openpanel-web/src/capability_registry.rs`
  is a typed 41-entry inventory (27 global + 14 site) with
  `global_capabilities` / `site_entry_for_tab`; the `capability-navigation`
  spec guards nav ↔ registry ↔ router agreement. Reuse it as the *shipped*
  cross-check; do not create a second registry.
- **OpenSpec specs** — `openspec/specs/` (83 live capabilities) is the source of
  truth for the *catalog*. The generator reads this directory; it does not
  hand-maintain a list.
- **Browser harness** — `tests/browser` pins Playwright 1.49.1 + axe-core and
  already drives a route matrix in `browser-ui-quality`; reuse it for capture
  if it can screenshot, rather than adding a second browser dependency.
- **Docs check** — `scripts/check-docs.sh` is the `make docs` leg. Extend it.
- **Self-tests** — `scripts/test-gates.sh` is the governance self-test harness;
  add positive + negative fixtures there for the new docs rule.
- **Dev bootstrap** — `cargo run -p openpanel-cli -- dev` is the documented
  no-setup boot; reuse it for capture.

## Runtime and Commands

- **Runtime:** Rust workspace; axum + tower; SQLite; maud HTMX web UI;
  `openpanel-web` is the UI crate.
- **Capture runtime:** `cargo run -p openpanel-cli -- dev` on
  `http://127.0.0.1:8080`, default `admin` owner on a throwaway data dir;
  captures use demo sites/DBs only.
- **Catalog runtime:** a small script reading `openspec/specs/*/spec.md`.
- **Verification runtime:** `make check` (full chain), `make test-gates`,
  the canonical Local Gate `make gate` (= `driftwatchdog gate`), and
  `openspec validate --strict`.

## Ownership

| Artifact | Owner |
|---|---|
| `README.md`, `docs/screenshots/*`, catalog page | `openpanel` maintainer |
| `scripts/check-docs.sh`, `scripts/test-gates.sh` fixtures | `openpanel` maintainer (extended here) |
| `capability_registry.rs` and `capability-navigation` spec | `openpanel` maintainer (reused, unchanged) |
| `openspec/governance/manifest.yaml` | Protected; **not** edited (no protected requirement changes) |
| `.project.json` declaration | `workspace-governance` vocabulary (read-only here) |
| Runtime readiness contract | `platform-contracts` (`platform.readiness`) — not this capability |

## Behavioral Contract

| ID | Behavior | Evidence |
|---|---|---|
| R1 | README embeds Web UI screenshots (login/shell, dashboard, site workspace, audit center, software center) under `docs/screenshots/` with captions and a no-secret note | Images render; `make check` docs leg passes |
| R2 | A capability catalog generated from `openspec/specs/` lists each live capability with its spec path, is linked from README, and matches the live spec set | Catalog regenerates clean; count matches `openspec/specs/`; cross-checked against the registry |
| R3 | The docs leg fails on a missing screenshot, a broken relative link, or a stale catalog; positive and negative fixtures exist | `make check` / `make test-gates` green on the tree, red on fixtures |
| R4 | README's `make check` chain description matches `Makefile`/`AGENTS.md` | Diffed against `Makefile`; stale gate names absent |

## Contract and Compatibility

- **Reader contract:** every in-repo relative README link resolves; external
  links are informational.
- **Spec truth:** the catalog is generated from `openspec/specs/`; the generator
  MUST NOT be the source of truth. Regeneration MUST be idempotent.
- **Registry reconciliation:** the catalog (all live specs) and the shipped
  registry (41 entries) may legitimately differ; the design records how the
  catalog is cross-checked without claiming the registry is incomplete.
- **Reuse ratchet:** the new generator must not introduce unclassified duplicate
  public items or bare names that trip `make reuse-strict`; follow the rename
  precedent from prior changes.
- **Governance compatibility:** no requirement in `agent-quality`, `quality`,
  `testing` or `architecture` is edited, so `openspec/governance/manifest.yaml`
  is unchanged and `make governance-contract` stays green.
- **Gate compatibility:** `make check` must stay green end-to-end; the new docs
  rule is fail-closed.

## Failure Policy

- If the panel cannot boot or the browser harness cannot capture, **record the
  blocker** and do not fabricate images.
- If a capture contains a secret, key, token or real host/user data, discard and
  re-capture from the dev bootstrap's synthetic state.
- If the catalog generator cannot reconcile with the live spec set, stop and
  record the discrepancy; never hand-edit the generated catalog to pass.
- The docs check remains fail-closed; a missing asset is a failed task, not a
  warning.
- Commit only this change's paths: the pre-existing dirty `openpanel-app`/
  `openpanel-domain` edits and the untracked `add-portable-host-operations-and-migration`
  folder MUST NOT be swept in.

## Oracle (exact commands)

```sh
# 1. Boot the panel for capture (throwaway data dir, default admin)
cargo run -p openpanel-cli -- dev         # http://127.0.0.1:8080/login

# 2. Regenerate the capability catalog (idempotent)
scripts/capabilities.sh > docs/CAPABILITIES.md   # name per L1
#   or the script name confirmed in the ledger

# 3. Docs leg
make docs

# 4. Full gate + governance self-tests
make check
make test-gates

# 5. Canonical Local Gate (resolve every pending review by running the
#    underlying script to completion)
make gate        # == driftwatchdog gate

# 6. Strict OpenSpec validation
openspec validate post-mvp-readiness --strict
```

Completion requires (4), (5) and (6) green with zero pending reviews plus every
`tasks.md` box ticked with dated evidence.

## Ledger (blockers, not guessed)

| # | Item | Why it blocks | Resolution sought |
|---|---|---|---|
| L1 | **Catalog source and name.** `openspec/specs/` holds 83 live capabilities while `capability_registry.rs` holds 41 shipped entries. Confirm which set the catalog enumerates and where the generator lives. | R2 must not invent a third registry or misstate shipped state. | Principal confirms catalog = live specs, with a registry cross-check column; confirms script path/name. |
| L2 | **Capture harness reuse.** Confirm `tests/browser` can screenshot arbitrary routes, or whether a manual/one-off capture path is needed. | Determines whether R1 is automated or manual. | Maintainer inspects the harness and records the decision. |
| L3 | **Demo UI state.** The dev bootstrap creates an owner but no sites/DBs/audit rows; screenshots need representative state. | Empty screens would misrepresent the product. | Principal confirms a synthetic seed path (sites, DB, an audit event) for captures. |
| L4 | **Image budget.** No screenshot size/count policy exists. | Repo size/clone time. | Principal confirms per-image cap and total budget. |
| L5 | **Docs-leg extension point.** Confirm `scripts/check-docs.sh` structure and how `test-gates.sh` fixtures bind to it. | R3 must extend the existing gate and self-test, not duplicate them. | Maintainer reads both scripts and records their current rules. |
| L6 | **README `make check` drift.** Confirm the current chain string against `Makefile`/`AGENTS.md` before editing R4. | Avoid asserting a wrong chain. | Maintainer records the authoritative chain. |
| L7 | **Dirty-tree isolation.** The repo has pre-existing modified files and an active untracked change. | The two commits must contain only this change's paths. | Principal confirms the staging boundary (this change's paths only). |

## Ledger resolutions (implementation record, 2026-09-30)

| # | Resolution |
|---|---|
| L1 | Catalog enumerates the **live specs** in `openspec/specs/` (98 at implementation time), not the shipped web registry. Generator is `scripts/capabilities.sh`; output is `docs/CAPABILITIES.md`. The registry cross-check is a "Shipped nav" column plus a reconciliation section, derived by grepping `capability: "..."` from `crates/openpanel-web/src/capability_registry.rs` — it is a reference, not a second registry. |
| L2 | Capture path is `shot-scraper` 1.12 (Playwright-driven, already installed on the maintainer host, using system Chrome via `-b chrome`). The committed `tests/browser` harness remains the CI axe gate; `shot-scraper` is the one-off capture path (no second browser dependency added to the repo). |
| L3 | Demo state is the `openpanel dev` bootstrap (owner `admin`) plus a synthetic seed: 3 `.example` sites (one disabled), 2 MySQL databases, and 11 audit rows across success/denied outcomes. No secret, token, or real host/user data is present; verified by visual review of every capture. |
| L4 | Budget: PNG, ≤ 400 KB per image, ≤ 2 MB total. The shipped set is 5 images, 376 KB total. |
| L5 | `scripts/check-docs.sh` was `cargo doc` only; it now runs `doc` plus `docs-readiness`. `--readiness-only` exists so `scripts/test-gates.sh` can exercise the readiness rule on a fixture tree without `cargo doc`. The shipped gate (and `driftwatchdog` `docs` check) invokes the script with no arguments, so both legs always run with no `.ai-gate/gate.yaml` change. |
| L6 | The README's `make check` chain had drifted (it omitted `ensure-lint-tools`, `browser-ui-quality`, `release-governance`, `release-evidence`, `portable-runtime`, `deployment-adapters`, `application-delivery`, `host-migration-interoperability`, `governance-contract`, `coverage-floor`, `maturity`, and `driftwatch-gate`). The chain in the "Spec-first development" and "Local checks" sections was rewritten to match `Makefile`/`AGENTS.md`. |
| L7 | Staging boundary: only this change's paths (`README.md`, `docs/CAPABILITIES.md`, `docs/screenshots/`, `scripts/capabilities.sh`, `scripts/check-docs.sh`, `scripts/test-gates.sh`, the archived change, the live `readiness` spec). The pre-existing `.project.json` edit and the untracked `.commandcode/` directory are left untouched. |