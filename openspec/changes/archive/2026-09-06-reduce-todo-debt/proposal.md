# Proposal: Triage and reduce TODO debt
## Why
There are 6 real TODO/FIXME markers across crates (e.g. crates/openpanel-web/src/audit.rs, monitoring.rs). They are minor debt but untracked, so they may never be resolved.
## What Changes
- Triage the 6 markers; fix the safe ones and convert the rest to tracked issues.
- Confirm clippy -D warnings is enforced to prevent new lint debt.

## Capabilities
### New Capabilities
- `reduce-todo-debt`: outstanding TODO/FIXME markers are tracked and reduced.

### Modified Capabilities
None.

## Impact
Affects: crates/*, ci configuration.
