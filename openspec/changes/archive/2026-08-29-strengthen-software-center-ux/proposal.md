# Strengthen Software Center trust and deployment UX

## Why

aaPanel and BaoTa make runtime managers, plugins, storage integrations, and one-click deployments central to the product. OpenPanel has a typed Software Center with previews and rollback concepts, but placeholder-digest and refresh-required states can make the catalog look broken or unsafe without enough explanation.

## What

Create a trust-forward catalog experience: source identity, signature/digest state, compatibility, permissions, dependencies, conflicts, estimated impact, preview, execute, progress, rollback, and clear recovery guidance.

## Capabilities

### New

- Catalog trust and provenance presentation.
- Compatibility and permissions summaries.
- Consistent transaction timeline and recovery UI.

### Modified

- Existing install/update/remove/deploy flows use the same preview and progress vocabulary.

## Non-goals

- No weakening of verified-digest gates.
- No arbitrary third-party installation.
- No new application recipe catalog in this change.

## Dependencies

Depends on `repair-ui-discoverability`; may link to audit/activity and dashboard job summaries.
