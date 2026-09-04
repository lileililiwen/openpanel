# Proposal: Ratchet archived governance requirements by content

## Why

`check-spec-drift.sh` currently proves only that an archived delta’s
requirement heading exists in the live spec. That positive result does not
prove the requirement text, scenarios, or executable protection survived a
later edit. Governance concerns are especially vulnerable because they can be
weakened while all ordinary product tests remain green.

## What

Introduce a committed manifest for archived governance obligations and a
content-level gate that verifies requirement blocks and their scenario counts
remain intact and map to executable checker IDs. Existing `check-spec-drift.sh`
remains the broad compatibility gate; this is a focused, stricter ratchet for
`agent-quality`, `quality`, `testing`, and `architecture` governance entries.

## Capabilities

### New

- `quality`: archived governance content and executable-protection ratchet.

### Modified

- `agent-quality`: archived governance requirements cannot be silently
  weakened.

## Non-goals

- No automatic merge of archived specs into live specs.
- No hashing of ordinary product requirements in this change.
- No claim that a matching text hash proves runtime behavior without the
  mapped executable check.
