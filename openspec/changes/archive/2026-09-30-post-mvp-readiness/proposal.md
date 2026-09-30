# Proposal: Post-MVP documentation readiness — Web UI screenshots and capability-catalog linkage

## Why

OpenPanel ships 83 live capabilities and 124 archived changes, a pure-Rust HTMX
Web UI, and a green `make check` gate — yet its `README.md` shows no screenshot
and points at `openspec/specs/` without ever listing what is there. The gap is
visibility, not capability: a reader cannot see the panel they are being asked
to trust, and cannot navigate a capability surface that has outgrown prose.
This change closes that documentation-readiness gap; it changes no runtime
behavior and adds no product capability.

## What Changes

- Add a **Web UI screenshot set** (login/shell, operations dashboard, site
  workspace, audit activity center, software center) under `docs/screenshots/`,
  embedded in README with captions and a demo-data/no-secret note.
- Add a **capability catalog** generated from `openspec/specs/`, listing each
  live capability with its spec path, and link it from README so the README's
  "83 capabilities" claim is verifiable and navigable rather than asserted.
- Wire a **documentation-readiness check** into the existing `make check` docs
  leg (reusing `scripts/check-docs.sh`) so a missing screenshot or a stale
  catalog fails the gate; add positive and negative fixtures.
- Correct the README's `make check` chain description if it drifts from
  `Makefile`/`AGENTS.md`.

## Package Boundary and Split Assessment

One repository-local documentation change:

- **One owner.** README, `docs/screenshots/`, the catalog and the docs check
  are owned by `openpanel`.
- **One lifecycle.** Generate catalog → capture screenshots → embed → check,
  with one evidence boundary (the running panel plus committed assets) and one
  acceptance oracle (`make check`, `openspec validate --strict`).
- **No second oracle.** No runtime, schema, API or migration change, so no
  runtime/contract package is needed.
- **Split trigger.** A portfolio-wide asset standard belongs to
  `workspace-governance`; changes to the `platform.readiness` runtime contract
  belong to `platform-contracts`. Neither is folded in here.

## Sibling and Shared Architecture Reconnaissance

| Sibling / shared surface | What it provides | Reuse for this change? | Decision (keep-local default) |
|---|---|---|---|
| `crates/openpanel-web/src/capability_registry.rs` + `capability-navigation` spec | Typed 41-entry capability inventory and nav/router agreement guard | **Yes** — cross-check the catalog against it; do not re-invent a registry | Reuse, keep-local |
| `tests/browser` (Playwright 1.49.1 + axe-core) | Pinned browser harness already used by `browser-ui-quality` | **Yes** — candidate capture harness for screenshots | Reuse, keep-local |
| `scripts/check-docs.sh` | Existing `make docs` leg | **Yes** — extend the docs leg rather than add a second gate | Reuse, keep-local |
| `platform-contracts` | `platform.readiness` runtime `/healthz` + `/readyz` schema | No — runtime health, not documentation readiness | Keep-local; do not overload the family |
| `workspace-governance` | `.project.json` vocabulary; openpanel is adopted, non-deployable | Read only — declaration unchanged | Keep-local |
| `openaccounting`, `mortalect`, `opendockify` | Same class of README evidence gap | No — separate repos and toolchains | Keep-local; separate change per repo |
| `rust-platform-libs` / `driftwatchdog` | Shared Rust gate runtime and evidence | Not extended here | Keep-local |

**Outcome:** keep-local. The repository already owns richer reuse surfaces
(registry, browser harness, docs check) than any sibling could provide.

## BFS Impact Map

- **Capabilities:** new `readiness`; no live capability modified.
- **Users:** prospective operators, contributors, and maintainers.
- **Contracts/data:** README, `docs/screenshots/*`, a generated catalog page,
  and the docs check. No spec, schema, API or config/data change.
- **Integrations:** the existing `cargo run -p openpanel-cli -- dev` bootstrap
  and the `tests/browser` harness; no external provider.
- **Failure boundaries:** missing/stale assets; secrets or real host data in an
  image; a catalog that drifts from `openspec/specs/`; broken links.
- **Unaffected:** runtime, API/CLI, migrations, the governance manifest
  (no protected requirement is edited), and the in-flight
  `add-portable-host-operations-and-migration` change.

## Capabilities

### New Capabilities

- `readiness`

### Modified Capabilities

- None.

## Non-goals

- No UI, API, CLI, schema, migration or capability behavior change.
- No browser-automation test change beyond reusing the existing harness.
- No governance-manifest edit (no protected requirement is modified).
- No touch to the in-flight `add-portable-host-operations-and-migration` change.
- No cross-repository documentation standard (see Package Boundary).