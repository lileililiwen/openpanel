# Refine specs with drift repair

## Why

Archived spec deltas were never reliably folded into
`openspec/specs/`. Building the archive-vs-live comparison gate
revealed the problem is systemic: **106 requirements across 11
capabilities** (sites, monitoring, backups, cron, dns, mail,
software-center, agent, quotas, plugin-extension-framework, ssl) exist
only in `openspec/changes/archive/`, while live specs were partially
rewritten at various points. Specs are the source of truth; un-merged
deltas mean the source of truth is stale and the Spec-To-Test Drift
Gate cannot see shipped behaviour.

## What Changes

- **Ratchet gate**: new `scripts/check-spec-drift.sh` compares every
  archived delta's ADDED/MODIFIED requirements against live specs.
  Pre-existing gaps are recorded in `openspec/specs/.drift-baseline`
  and reported without failing; any NEW drift fails `make check`
  immediately. Wired into the Makefile as `spec-drift`.
- **Two merges landed now** (unambiguous, no supersession risk):
  DNSSEC/secondary-DNS (5 requirements) into `dns`, and database
  privilege management (3 requirements) into `databases` — shrinking
  the baseline by 8.
- Remaining baseline entries are tracked debt: each capability needs a
  human-reviewed pass to distinguish "never merged" from "superseded
  by later rewrite" before merging — blind mass-merging would
  resurrect obsolete requirements.

## Capabilities

### Modified Capabilities

- `quality`: add the archive-time delta-merge gate (ratchet model).
- `dns`, `databases`: receive the merged requirements (verbatim folds
  of already-approved deltas).

## Impact

- Scripts + specs only; no runtime code. Baseline file committed so CI
  and local runs agree.
- Unblocks `refine-databases-with-remote-access-enforcement`, which
  modifies requirements that existed only in an archived delta.

## Non-goals

- Merging the remaining ~98 baseline entries in this change (needs
  per-capability supersession review).
- mail_filtering merge is owned by `refine-mail-with-user-surfaces`.
