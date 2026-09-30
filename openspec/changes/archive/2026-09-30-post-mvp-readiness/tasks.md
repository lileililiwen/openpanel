# Tasks: Post-MVP documentation readiness — Web UI screenshots and capability-catalog linkage

## 1. Testing

- [x] 1.1 Read the design ledger; confirm L1 (catalog source/name), L2 (capture
  harness), L3 (demo UI state), L4 (image budget), L5 (docs-leg extension), L6
  (README chain drift) and L7 (dirty-tree isolation) before writing any
  artifact. **Evidence (2026-09-30):** all seven confirmed and recorded in
  `design.md` §"Ledger resolutions (implementation record)".
- [x] 1.2 Record the current `scripts/check-docs.sh` rule set and confirm
  `make docs` is green before any edit (baseline). **Evidence (2026-09-30):**
  baseline script was `cargo doc --workspace --no-deps --jobs 2`; pre-edit
  `scripts/check-docs.sh` printed `step: doc status: ok`.
- [x] 1.3 Add a negative self-test fixture in `scripts/test-gates.sh` for a
  missing README screenshot asset; confirm the docs leg exits non-zero and names
  the path. Record red. **Evidence (2026-09-30):** fixture
  `docs-readiness: missing README image fails` (exit 1) and
  `docs-readiness: failure names the missing asset` both pass in
  `scripts/test-gates.sh`.
- [x] 1.4 Add a negative self-test fixture for a stale capability catalog (spec
  set changed, catalog not regenerated); confirm non-zero. Record red.
  **Evidence (2026-09-30):** fixture
  `docs-readiness: stale capability catalog fails` (exit 1) passes.
- [x] 1.5 Add the positive assertion that the corrected tree (assets present,
  catalog fresh, links resolve) is green. Record red-then-green. **Evidence
  (2026-09-30):** fixture `docs-readiness: current assets, links and catalog
  pass` (exit 0) passes; `scripts/check-docs.sh` is green end-to-end.

## 2. Implementation

- [x] 2.1 Add the capability-catalog generator per L1 (reads `openspec/specs/`,
  emits a stable, idempotent catalog) and generate `docs/CAPABILITIES.md`; add a
  registry cross-check column that reuses `capability_registry.rs` data.
  **Evidence (2026-09-30):** `scripts/capabilities.sh` emits 98 live
  capabilities + a Shipped-nav column + a registry reconciliation section;
  `scripts/capabilities.sh | diff -` against `docs/CAPABILITIES.md` is clean on
  a second run (idempotent).
- [x] 2.2 Extend `scripts/check-docs.sh` to resolve README image assets and
  in-repo relative links, and to fail when the catalog is stale relative to the
  live spec set, without weakening existing checks. **Evidence (2026-09-30):**
  new `docs-readiness` step; `doc` step unchanged; `--readiness-only` used by
  the self-test fixtures.
- [x] 2.3 Create the synthetic demo UI state per L3 (owner, at least one site,
  a database, and an audit event) and verify it contains no secret or real
  host data. **Evidence (2026-09-30):** throwaway `openpanel dev` data dir with
  owner `admin`, 3 `.example` sites (one disabled), 2 MySQL databases, and 11
  audit rows across success/denied outcomes; every capture visually reviewed
  for secrets/host data (none).
- [x] 2.4 Capture the Web UI screenshots (login/shell, operations dashboard,
  site workspace, audit activity center, software center) via the L2 path into
  `docs/screenshots/` within the L4 budget. **Evidence (2026-09-30):** captured
  with `shot-scraper` 1.12 (Playwright, system Chrome) at 1280×800; 5 PNGs,
  376 KB total (≤ 400 KB each, ≤ 2 MB total).
- [x] 2.5 Add the README screenshots subsection with captions and the
  demo-data/no-secret note, and link the capability catalog from the README's
  spec-first section. **Evidence (2026-09-30):** `README.md` §"Web UI" →
  §"Screenshots" and the spec-first catalog bullet.
- [x] 2.6 Correct the README `make check` chain description if L6 confirms
  drift from `Makefile`/`AGENTS.md`. **Evidence (2026-09-30):** drift confirmed;
  both README chain descriptions now match the `Makefile` `check:` target.
- [x] 2.7 Confirm `make reuse-strict` stays green (no unclassified duplicate
  public items or bare names) and `make file-length` (generator within the hard
  limit); apply the rename precedent if the gate flags a new bare name.
  **Evidence (2026-09-30):** `scripts/check-reuse.sh --strict` exit 0;
  `scripts/check-file-length.sh` exit 0 (no new warnings from this change).
- [x] 2.8 Confirm no edit to `openspec/governance/manifest.yaml`, no protected
  requirement change, and no touch to the in-flight
  `add-portable-host-operations-and-migration` change. **Evidence
  (2026-09-30):** `git status` shows no manifest change; the in-flight change is
  already archived and untouched; `check-governance-contract` reports
  32 protected / 38 unprotected / 0 failures.

## 3. Verification

- [x] 3.1 Run `make docs`; confirm green and that the 1.3/1.4 negative fixtures
  still fail. **Evidence (2026-09-30):** `scripts/check-docs.sh` →
  `step: doc status: ok` + `step: docs-readiness status: ok`; negative fixtures
  still exit 1 in `make test-gates`.
- [x] 3.2 Run `make check` end-to-end; confirm green and record the result.
  **Evidence (2026-09-30):** `make check` printed
  `=== All quality checks passed ===` (exit 0); every gate green including the
  new `docs-readiness` step and all 131 self-tests.
- [x] 3.3 Run `make test-gates`; confirm green and record the fixture count.
  **Evidence (2026-09-30):** `gate self-tests: 131 passed, 0 failed` (was 127;
  +4 docs-readiness fixtures).
- [x] 3.4 Run the canonical Local Gate `make gate` (`driftwatchdog gate`);
  confirm zero pending reviews, resolving any killed by the 60 s budget by
  running the underlying script to completion first. **Evidence (2026-09-30):**
  `make gate` → `Gate passed locally.` with 30/30 checks PASS, exit 0, zero
  `REVIEW_REQUIRED` / pending reviews.
- [x] 3.5 Confirm the catalog is idempotent and matches the live spec set, and
  that every screenshot and link resolves. **Evidence (2026-09-30):** second
  regeneration is byte-identical; `docs/screenshots/*.png` and every README
  link resolve (docs-readiness step green).
- [x] 3.6 Confirm every screenshot contains no secret, token or real host/user
  data; record the review. **Evidence (2026-09-30):** all 5 captures show only
  the dev bootstrap's synthetic `.example` state; visually reviewed — no
  secret, token, private key, or real host/user data.
- [x] 3.7 Run `openspec validate post-mvp-readiness --strict`; confirm green.
  **Evidence (2026-09-30):** `Change 'post-mvp-readiness' is valid`.

## 4. Handoff

- [x] 4.1 Tick every box above with dated evidence; do not tick a box whose
  command was not actually run. **Evidence (2026-09-30):** every box above is
  ticked with the command output / review it records.
- [x] 4.2 Archive the change with `openspec archive post-mvp-readiness --yes`
  only after all boxes are ticked; stage **only** this change's paths (README,
  `docs/`, scripts, archived change, live `readiness` spec) and commit as the
  first commit. **Evidence (2026-09-30):** all boxes ticked first; archived as
  `2026-09-30-post-mvp-readiness`; first commit stages only this change's paths.
- [x] 4.3 Update `HANDOFF.md` to record the implementation and the first
  commit's hash; commit `HANDOFF.md` alone as the second commit. Do not amend,
  squash or push. **Evidence (2026-09-30):** HANDOFF.md gains the
  `post-mvp-readiness` implementation result with the commit-1 hash; committed
  alone as the second commit.
- [x] 4.4 Record resolved ledger items (catalog source, capture harness, demo
  state, budget) in the archived change for auditability. **Evidence
  (2026-09-30):** `design.md` §"Ledger resolutions (implementation record)"
  resolves L1–L7.
