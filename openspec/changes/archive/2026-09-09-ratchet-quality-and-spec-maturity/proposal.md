# Proposal: Ratchet quality and spec maturity

## Why

OpenPanel has a broad governance chain, but several important checks remain
warning-only or informational. The current spec-test drift gate reports
uncovered capabilities without failing, coverage can emit a stub report, and
reuse reports duplicated public items without failing by default. This lets a
new feature appear complete while its requirements, tests, and production
proof diverge.

## What Changes

- Make spec-test coverage strict for new and modified capabilities.
- Add an explicit, shrinking baseline for pre-existing uncovered scenarios.
- Add enforceable coverage thresholds and fail when the coverage tool is
  unavailable in required CI jobs.
- Add a strict duplicate-public-item mode to the mandatory quality chain.
- Require tracked evidence for stubs, TODOs, and environment-blocked checks.
- Add positive and negative governance self-tests for each new gate.

## Capabilities

### New Capabilities

- `quality-maturity-ratchet`: quality evidence and known debt are ratcheted.

### Modified Capabilities

- `quality`
- `testing`
- `agent-quality`

## Non-goals

- No product feature implementation.
- No mass refactor of existing duplicate functions in this change.
- No deletion of legitimate test fakes or fixture-only placeholders.
- No weakening of archived governance digests.

## Dependencies

This is the first change in the sequence. Release and product changes depend
on its stricter verification vocabulary.
