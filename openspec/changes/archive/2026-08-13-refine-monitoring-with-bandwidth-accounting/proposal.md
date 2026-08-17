# Refine monitoring with bandwidth accounting and quota thresholds

## Why

`openspec/specs/monitoring/spec.md` describes CPU/Memory/Disk/
Network metric kinds and `SiteTrafficInsights` for the `logs`
capability. It does not cover **per-account/site/per-period
bandwidth accounting** with rolling-window counters and quota
thresholds — the data backbone that the proposed
`add-resource-quotas` and `add-notification-channels` changes
need. This refinement introduces the typed counter shape and the
threshold-crossing emission contract; the storage layer lands in
the new `add-bandwidth-accounting` change.

## What Changes

- New value objects: `BandwidthWindow { Span, BytesIn, BytesOut }`
  and `BandwidthCounter { OwnerId, PeriodKind, … }`.
- New domain events: `BandwidthThresholdCrossed`,
  `BandwidthWindowClosed`.
- New collector plugin point: `BandwidthObserver` is called on
  every byte transition through the existing `monitoring` runtime.

## Capabilities

### Modified Capabilities

- `monitoring`: bandwidth accounting events, counter shape, and
  collector plugin contract.

## Impact

- Domain: `BandwidthCounter`, `BandwidthWindow`, value objects
  under `crates/openpanel-domain/src/monitoring/`.
- App: `BandwidthObserver` trait in
  `crates/openpanel-app/src/monitoring/`.
- No storage impact in this change; the follow-on
  `add-bandwidth-accounting` change persists it.
